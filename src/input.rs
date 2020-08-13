use std::io;

pub fn exists() -> bool {
    #[cfg(test)]
    {
        stub::stdin_data().get_mut().is_some()
    }
    #[cfg(not(test))]
    {
        atty::isnt(atty::Stream::Stdin)
    }
}

pub fn read_to_string(buf: &mut String) -> io::Result<usize> {
    #[cfg(test)]
    {
        if let Some(ref data) = stub::stdin_data().get_mut() {
            buf.push_str(data.as_ref());
            Ok(data.len())
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "Stub stdin data is not set",
            ))
        }
    }
    #[cfg(not(test))]
    {
        use std::io::Read;
        io::stdin().read_to_string(buf)
    }
}

#[cfg(test)]
#[must_use]
pub fn set_stdin_data(data: Option<String>) -> stub::Handle {
    *stub::stdin_data().get_mut() = data;
    stub::Handle {}
}

#[cfg(test)]
mod stub {
    use once_cell::sync::Lazy;

    use std::cell::RefCell;
    use std::sync::{Arc, Mutex, MutexGuard};

    pub static STDIN_STUB: Lazy<Arc<Mutex<RefCell<Option<String>>>>> =
        Lazy::new(|| Arc::new(Mutex::new(RefCell::new(None))));

    pub struct Handle {}

    impl Drop for Handle {
        fn drop(&mut self) {
            *STDIN_STUB.lock().unwrap().get_mut() = None;
        }
    }

    pub fn stdin_data() -> MutexGuard<'static, RefCell<Option<String>>> {
        STDIN_STUB.lock().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_handle() {
        let handle = set_stdin_data(Some("x".to_owned()));
        assert!(stub::STDIN_STUB.lock().unwrap().get_mut().is_some());
        drop(handle);
        assert!(stub::STDIN_STUB.lock().unwrap().get_mut().is_none());
    }

    #[test]
    fn stubbed_stdin() {
        {
            let mut buf = String::new();
            assert!(read_to_string(&mut buf).is_err());
        }
        {
            let _handle = set_stdin_data(Some("x".to_owned()));
            let mut buf = String::new();
            assert!(read_to_string(&mut buf).is_ok());
            assert_eq!(buf, "x");
        }
    }
}
