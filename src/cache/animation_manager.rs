use std::io::Read as _;
use std::sync::atomic::{AtomicBool, Ordering};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use gtk::glib;

use super::{AnimationEntry, SharedAnimationEntry};
enum AnimationManagerEntry {
    Uninitialized(Vec<Box<dyn Fn(SharedAnimationEntry) + Send>>),
    Initialized(SharedAnimationEntry),
}

impl AnimationManagerEntry {
    fn new() -> Self {
        Self::Uninitialized(vec![])
    }

    fn push_callback<F>(&mut self, callback: F)
    where
        F: Fn(SharedAnimationEntry) + 'static + Send,
    {
        match self {
            AnimationManagerEntry::Initialized(entry) => callback(entry.clone()),
            AnimationManagerEntry::Uninitialized(callbacks) => callbacks.push(Box::new(callback)),
        }
    }

    fn skip_init(&self) -> bool {
        match self {
            AnimationManagerEntry::Uninitialized(callbacks) => callbacks.len() > 1,
            AnimationManagerEntry::Initialized(_) => true,
        }
    }

    fn init(&mut self, animation: rlottie::Animation) {
        let entry = Arc::new(Mutex::new(AnimationEntry::new(animation)));
        if let Self::Uninitialized(callbacks) = self {
            for callback in callbacks {
                (*callback)(entry.clone())
            }
        } else {
            panic!("already initialized")
        }
        *self = Self::Initialized(entry)
    }

    fn tick(&self) {
        if let Self::Initialized(entry) = self {
            entry.lock().unwrap().tick();
        }
    }
}

#[derive(Default)]
pub(crate) struct AnimationManager {
    entries: Arc<Mutex<HashMap<String, AnimationManagerEntry>>>,
    initialized: AtomicBool,
}

impl AnimationManager {
    pub(crate) fn open_animation<F>(&self, path: &str, callback: F)
    where
        F: Fn(SharedAnimationEntry) + 'static + Send,
    {
        {
            let mut entries = self.entries.lock().unwrap();

            let entry = entries
                .entry(path.to_owned())
                .or_insert_with(AnimationManagerEntry::new);

            entry.push_callback(callback);

            if entry.skip_init() {
                return;
            }
        }

        let entries = self.entries.clone();
        let path = path.to_owned();

        std::thread::spawn(move || {
            let animation = {
                match rlottie::Animation::from_file(&path) {
                    Some(animation) => animation,
                    _ => {
                        if let Ok(data) = std::fs::read(&path) {
                            let mut buf = String::new();
                            let mut gz = flate2::bufread::GzDecoder::new(&data[..]);
                            if gz.read_to_string(&mut buf).is_ok() {
                                rlottie::Animation::from_data(buf, path.clone(), "")
                                    .expect("LottieAnimationPaintable: unsupporded file type")
                            } else {
                                unimplemented!("LottieAnimationPaintable: unsupporded file type")
                            }
                        } else {
                            unimplemented!("LottieAnimationPaintable: file not found")
                        }
                    }
                }
            };

            entries
                .lock()
                .unwrap()
                .get_mut(&path)
                .unwrap()
                .init(animation);
        });
    }

    pub(crate) fn init(&self) {
        if !self.initialized.fetch_or(true, Ordering::Acquire) {
            let animations = self.entries.clone();
            glib::timeout_add(std::time::Duration::from_secs_f64(1.0 / 60.0), move || {
                for entry in animations.lock().unwrap().values() {
                    entry.tick()
                }
                glib::Continue(true)
            });
        }
    }
}
