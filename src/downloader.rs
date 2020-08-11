use std::fs::{self, File};
use std::io::{self, Write};
use std::mem;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use curl::easy::{Easy2, Handler, WriteError};
use curl::multi::Multi;
use url::Url;

use crate::database::Photoset;

const MAX_CONCURRENCY: usize = 4;

pub type OnDownloadedPhotoset = Box<dyn Fn(&Photoset)>;

pub struct Downloader {
    on_downloaded_photoset: OnDownloadedPhotoset,
    single_photo_photosets: Vec<Photoset>,
    multi_photo_photosets: Vec<Photoset>,
}

impl Downloader {
    pub fn new(photosets: Vec<Photoset>, on_downloaded_photoset: OnDownloadedPhotoset) -> Self {
        let (single_photo_photosets, multi_photo_photosets) =
            photosets.into_iter().partition(|s| s.photo_urls.len() == 1);
        Downloader {
            on_downloaded_photoset,
            single_photo_photosets,
            multi_photo_photosets,
        }
    }

    pub fn start(&self) -> Result<()> {
        log::trace!("downloading single-photo photosets");
        self.download_single_photo_photosets()
            .context("Error occurred while downloading single-file photosets")?;
        log::trace!("downloading multi-photo photosets");
        self.download_multi_photo_photosets()
            .context("Error occurred while downloading multi-file photosets")?;
        Ok(())
    }

    fn download_single_photo_photosets(&self) -> Result<()> {
        fn add_jobs<'p>(
            multi: &Multi,
            handles: &mut Vec<(curl::multi::Easy2Handle<FileWriter>, &'p Photoset)>,
            single_sets_iter: &mut impl Iterator<Item = &'p Photoset>,
        ) -> Result<bool> {
            let mut added = false;
            for _ in 0..MAX_CONCURRENCY.saturating_sub(handles.len()) {
                if let Some(single_set) = single_sets_iter.next() {
                    let path = build_photo_path(&single_set, &single_set.photo_urls[0], 1);
                    let mut easy2 = Easy2::new(FileWriter::new(path));
                    easy2.get(true)?;
                    easy2.url(&single_set.photo_urls[0])?;
                    let handle = multi.add2(easy2)?;
                    log::trace!("added download job; url={}", &single_set.photo_urls[0]);
                    handles.push((handle, single_set));
                    added = true;
                } else {
                    break;
                }
            }
            Ok(added)
        }

        let multi = Multi::new();
        let mut handles = vec![];
        let mut single_sets_iter = self.single_photo_photosets.iter().peekable();

        loop {
            add_jobs(&multi, &mut handles, &mut single_sets_iter)?;
            let transfers_in_progress = multi.perform()?;
            multi.messages(|message| {
                let mut i = 0;
                while i < handles.len() {
                    let (handle, photoset) = &mut handles[i];
                    if let Some(result) = message.result_for2(&handle) {
                        if result.is_ok() {
                            if let Err(e) = handle.get_mut().finish() {
                                log::debug!("failed to write output file; error={:?}", e);
                            } else {
                                (self.on_downloaded_photoset)(photoset);
                            }
                        } else {
                            log::debug!(
                                "transfer failed; error={:?}; io_result={:?}",
                                result.unwrap_err(),
                                handle.get_ref().io_result,
                            );
                        }
                        // Drop handle to close file.
                        let (handle, _photoset) = handles.remove(i);
                        let _ = multi.remove2(handle);
                        // The elements after i has been shifted. Continue from i.
                        continue;
                    }
                    i += 1;
                }
            });
            if transfers_in_progress == 0 && single_sets_iter.peek().is_none() {
                break;
            }
            multi.wait(&mut [], Duration::from_secs(1))?;
        }

        Ok(())
    }

    fn download_multi_photo_photosets(&self) -> Result<()> {
        'each_multi_set: for multi_set in self.multi_photo_photosets.iter() {
            let multi = Multi::new();
            let mut handles = vec![];

            for (index, photo_url) in (1..).zip(multi_set.photo_urls.iter()) {
                let path = build_photo_path(multi_set, &photo_url, index);
                let mut easy2 = Easy2::new(FileWriter::new(path));
                easy2.get(true)?;
                easy2.url(&photo_url)?;
                let handle = multi.add2(easy2)?;
                log::trace!("added download job; url={}", &photo_url);
                handles.push(handle);
            }

            loop {
                let transfers_in_progress = multi.perform()?;
                let mut any_transfer_failed = false;
                multi.messages(|message| {
                    if let Some(Err(e)) = message.result() {
                        any_transfer_failed = true;
                        log::debug!("transfer failed; error={:?}", e);
                    }
                });
                if any_transfer_failed {
                    for handle in handles.into_iter() {
                        multi.remove2(handle)?;
                    }
                    continue 'each_multi_set;
                }
                if transfers_in_progress == 0 {
                    break;
                }
                multi.wait(&mut [], Duration::from_secs(1))?;
            }

            let mut all_finish_succeeds = true;
            for mut handle in handles.into_iter() {
                if let Err(e) = handle.get_mut().finish() {
                    all_finish_succeeds = false;
                    log::debug!("failed to write output file; error={:?}", e);
                };
                multi.remove2(handle)?;
            }
            if all_finish_succeeds {
                (self.on_downloaded_photoset)(multi_set);
            }
        }

        Ok(())
    }
}

struct FileWriter {
    file: FileWriterFile,
    io_result: io::Result<()>,
}

impl Handler for FileWriter {
    fn write(&mut self, data: &[u8]) -> Result<usize, WriteError> {
        match self.write_to_file(data) {
            Some(n) => Ok(n),
            None => {
                log::debug!("curl write error: {:?}", self.io_result);
                // Signal error by returning a different number than the size of the data passed.
                Ok(data.len().overflowing_sub(1).0)
            }
        }
    }
}

impl FileWriter {
    pub fn new(path: PathBuf) -> Self {
        FileWriter {
            file: FileWriterFile::Unopened { dest_path: path },
            io_result: Ok(()),
        }
    }

    pub fn write_to_file(&mut self, data: &[u8]) -> Option<usize> {
        if self.io_result.is_err() {
            return None;
        }
        match self.file().and_then(|f| f.write(data)) {
            Ok(n) => Some(n),
            Err(e) => {
                self.io_result = io::Result::Err(e);
                None
            }
        }
    }

    pub fn finish(&mut self) -> io::Result<()> {
        if self.io_result.is_err() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Attempted to finish incomplete file",
            ));
        }

        let mut file = FileWriterFile::Closed;
        mem::swap(&mut file, &mut self.file);
        if let FileWriterFile::Opened {
            part_file,
            part_path,
            dest_path,
        } = file
        {
            drop(part_file);
            fs::rename(part_path, dest_path)?;
        }
        Ok(())
    }

    pub fn discard_part(&mut self) -> io::Result<()> {
        let mut file = FileWriterFile::Closed;
        mem::swap(&mut file, &mut self.file);
        if let FileWriterFile::Opened {
            part_file,
            part_path,
            ..
        } = file
        {
            drop(part_file);
            fs::remove_file(part_path)?;
        }

        Ok(())
    }

    fn file(&mut self) -> io::Result<&mut File> {
        use FileWriterFile::*;

        if let Unopened { dest_path } = &self.file {
            let part_path = make_part_path(dest_path)?;
            let part_file = File::create(&part_path)?;
            self.file = Opened {
                part_file,
                part_path,
                dest_path: dest_path.clone(),
            };
        }

        match self.file {
            Closed => Err(io::Error::new(
                io::ErrorKind::Other,
                "Attempted to use closed file",
            )),
            Opened {
                ref mut part_file, ..
            } => Ok(part_file),
            Unopened { .. } => unreachable!(),
        }
    }
}

impl Drop for FileWriter {
    fn drop(&mut self) {
        let _ignore_error = self.discard_part();
    }
}

enum FileWriterFile {
    Closed,
    Opened {
        part_file: File,
        part_path: PathBuf,
        dest_path: PathBuf,
    },
    Unopened {
        dest_path: PathBuf,
    },
}

pub fn build_photo_path(photoset: &Photoset, photo_url: &str, index: usize) -> PathBuf {
    let url = Url::parse(photo_url).expect("photo_url must be valid");
    let name = url
        .path_segments()
        .and_then(|segs| segs.last())
        .expect("photo_url must have filename");
    PathBuf::from(format!(
        "@{}-{}-img{}-{}",
        photoset.screen_name, photoset.id_str, index, name
    ))
}

fn make_part_path(path: &PathBuf) -> io::Result<PathBuf> {
    let mut file_name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Destination path lacks file name"))?
        .to_owned();
    file_name.push(".part");
    Ok(path.with_file_name(file_name))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::make_part_path;

    #[test]
    fn part_path() {
        {
            let path = PathBuf::from("/foo/dest.txt");
            let part_path = PathBuf::from("/foo/dest.txt.part");

            assert_eq!(make_part_path(&path).unwrap(), part_path);
        }

        {
            // May be surprising but this is how PathBuf::file_name works.
            let path = PathBuf::from("/foo/");
            let part_path = PathBuf::from("/foo.part");

            assert_eq!(make_part_path(&path).unwrap(), part_path);
        }

        {
            let path = PathBuf::from("/");

            assert!(make_part_path(&path).is_err());
        }
    }
}

#[cfg(test)]
mod file_writer_tests {
    use std::fs;
    use std::io;
    use std::path::PathBuf;

    use tempfile::tempdir;

    use super::{make_part_path, FileWriter};

    #[test]
    fn new() {
        let temp = tempdir().unwrap();
        let dest_path = temp.path().join("dest.txt");
        let part_path = make_part_path(&dest_path).unwrap();

        let _writer = FileWriter::new(dest_path.clone());

        assert!(is_not_found(&dest_path));
        assert!(is_not_found(&part_path));
    }

    #[test]
    fn write() {
        let temp = tempdir().unwrap();
        let dest_path = temp.path().join("dest.txt");
        let part_path = make_part_path(&dest_path).unwrap();

        let mut writer = FileWriter::new(dest_path.clone());
        assert_eq!(writer.write_to_file(b"hello"), Some(5));

        assert!(is_not_found(&dest_path));
        assert_eq!(fs::read_to_string(&part_path).unwrap(), "hello");
    }

    #[test]
    fn write_and_finish() {
        let temp = tempdir().unwrap();
        let dest_path = temp.path().join("dest.txt");
        let part_path = make_part_path(&dest_path).unwrap();

        let mut writer = FileWriter::new(dest_path.clone());
        writer.write_to_file(b"hello").unwrap();
        assert!(writer.finish().is_ok());

        assert_eq!(fs::read_to_string(&dest_path).unwrap(), "hello");
        assert!(is_not_found(&part_path));
        assert!(writer.io_result.is_ok());
    }

    #[test]
    fn write_and_discard_part() {
        let temp = tempdir().unwrap();
        let dest_path = temp.path().join("dest.txt");
        let part_path = make_part_path(&dest_path).unwrap();

        let mut writer = FileWriter::new(dest_path.clone());
        writer.write_to_file(b"hello").unwrap();
        assert!(writer.discard_part().is_ok());

        assert!(is_not_found(&dest_path));
        assert!(is_not_found(&part_path));
        assert!(writer.io_result.is_ok());
    }

    #[test]
    fn write_and_drop() {
        let temp = tempdir().unwrap();
        let dest_path = temp.path().join("dest.txt");
        let part_path = make_part_path(&dest_path).unwrap();

        let mut writer = FileWriter::new(dest_path.clone());
        writer.write_to_file(b"hello").unwrap();
        drop(writer);

        assert!(is_not_found(&dest_path));
        assert!(is_not_found(&part_path));
    }

    #[test]
    fn finish_without_write() {
        let temp = tempdir().unwrap();
        let dest_path = temp.path().join("dest.txt");
        let part_path = make_part_path(&dest_path).unwrap();

        let mut writer = FileWriter::new(dest_path.clone());
        assert!(writer.finish().is_ok());

        assert!(is_not_found(&dest_path));
        assert!(is_not_found(&part_path));
        assert!(writer.io_result.is_ok());
    }

    #[test]
    fn finish_and_then_write() {
        let temp = tempdir().unwrap();
        let dest_path = temp.path().join("dest.txt");
        let part_path = make_part_path(&dest_path).unwrap();

        let mut writer = FileWriter::new(dest_path.clone());

        writer.write_to_file(b"hello").unwrap();
        writer.finish().unwrap();
        assert!(writer.write_to_file(b" world").is_none());

        assert_eq!(fs::read_to_string(&dest_path).unwrap(), "hello");
        assert!(is_not_found(&part_path));
        assert_eq!(
            writer.io_result.as_ref().unwrap_err().kind(),
            io::ErrorKind::Other
        );
    }

    #[test]
    fn finish_and_then_discard_part() {
        let temp = tempdir().unwrap();
        let dest_path = temp.path().join("dest.txt");
        let part_path = make_part_path(&dest_path).unwrap();

        let mut writer = FileWriter::new(dest_path.clone());

        writer.write_to_file(b"hello").unwrap();
        writer.finish().unwrap();
        assert!(writer.discard_part().is_ok());

        assert_eq!(fs::read_to_string(&dest_path).unwrap(), "hello");
        assert!(is_not_found(&part_path));
        assert!(writer.io_result.is_ok());
    }

    #[test]
    fn discard_part_and_then_write() {
        let temp = tempdir().unwrap();
        let dest_path = temp.path().join("dest.txt");
        let part_path = make_part_path(&dest_path).unwrap();

        let mut writer = FileWriter::new(dest_path.clone());

        writer.write_to_file(b"hello").unwrap();
        writer.discard_part().unwrap();
        assert!(writer.write_to_file(b" world").is_none());

        assert!(is_not_found(&dest_path));
        assert!(is_not_found(&part_path));
        assert_eq!(
            writer.io_result.as_ref().unwrap_err().kind(),
            io::ErrorKind::Other
        );
    }

    #[test]
    fn discard_part_and_then_finish() {
        let temp = tempdir().unwrap();
        let dest_path = temp.path().join("dest.txt");
        let part_path = make_part_path(&dest_path).unwrap();

        let mut writer = FileWriter::new(dest_path.clone());

        writer.write_to_file(b"hello").unwrap();
        writer.discard_part().unwrap();
        assert!(writer.finish().is_ok());

        assert!(is_not_found(&dest_path));
        assert!(is_not_found(&part_path));
        assert!(writer.io_result.is_ok());
    }

    fn is_not_found(path: &PathBuf) -> bool {
        match fs::metadata(path) {
            Err(e) => e.kind() == io::ErrorKind::NotFound,
            _ => false,
        }
    }
}
