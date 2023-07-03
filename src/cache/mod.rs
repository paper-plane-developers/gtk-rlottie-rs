mod animation_entry;
mod animation_info;
mod animation_manager;
mod frame_collection;
mod frame_request;
mod unsafe_send;

use std::sync::{Arc, Mutex, OnceLock};

pub(crate) use animation_entry::AnimationEntry;
pub(crate) use animation_manager::AnimationManager;

pub(crate) type SharedAnimationEntry = Arc<Mutex<AnimationEntry>>;

static ANIMATION_MANAGER: OnceLock<AnimationManager> = OnceLock::new();

pub(crate) fn open_animation<F>(path: &str, callback: F)
where
    F: Fn(SharedAnimationEntry) + 'static + Send,
{
    ANIMATION_MANAGER
        .get_or_init(|| {
            let manager = AnimationManager::default();
            manager.init();
            manager
        })
        .open_animation(path, callback);
}
