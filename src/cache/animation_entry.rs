use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex},
};

use gtk::{gdk, glib};

use super::{
    animation_info::AnimationInfo,
    frame_collection::FrameCollection,
    frame_request::{FrameRequest, FrameRequestIndex},
    unsafe_send::UnsafeSend,
};

#[derive(Debug)]
pub(crate) struct AnimationEntry {
    animation: Arc<Mutex<UnsafeSend<rlottie::Animation>>>,
    info: AnimationInfo,
    frame_collections: BTreeMap<(usize, usize), FrameCollection>,
    requests: VecDeque<FrameRequest>,
    processing: VecDeque<FrameRequest>,

    receiver: std::sync::mpsc::Receiver<(FrameRequestIndex, gdk::MemoryTexture)>,
    sender: std::sync::mpsc::Sender<(FrameRequestIndex, gdk::MemoryTexture)>,
}

impl AnimationEntry {
    pub(crate) fn new(animation: rlottie::Animation) -> Self {
        let (sender, receiver) = std::sync::mpsc::channel();

        let size = animation.size();
        let animation_info = AnimationInfo {
            frame_delay: std::time::Duration::from_secs_f64(1.0 / animation.framerate()),
            totalframe: animation.totalframe(),
            default_size: (size.width as i32, size.height as i32),
        };

        Self {
            animation: Arc::new(Mutex::new(UnsafeSend(animation))),
            info: animation_info,
            frame_collections: BTreeMap::new(),
            requests: VecDeque::new(),
            processing: VecDeque::new(),
            receiver,
            sender,
        }
    }

    pub(crate) fn request_frame(&mut self, width: usize, height: usize, index: usize) {
        let collection = self
            .frame_collections
            .entry((width, height))
            .or_insert_with(|| FrameCollection::new(self.info.totalframe));

        if collection.frame(index).is_none() {
            let request = FrameRequest {
                size: (width, height),
                frame_num: index,
                callback: None,
            };

            if !self.requests.contains(&request) {
                self.requests.push_back(request);
            }
        }
    }

    pub(crate) fn request_frame_with_callback<F>(
        &mut self,
        width: usize,
        height: usize,
        index: usize,
        callback: F,
    ) where
        F: Fn(&gdk::MemoryTexture) + Send + 'static,
    {
        let collection = self
            .frame_collections
            .entry((width, height))
            .or_insert_with(|| FrameCollection::new(self.info.totalframe));

        if let Some(texture) = collection.frame(index) {
            callback(&texture);
        } else {
            let request = FrameRequest {
                size: (width, height),
                frame_num: index,
                callback: Some(Box::new(callback)),
            };

            self.requests.push_back(request);
        }
    }

    pub(crate) fn frame(
        &self,
        width: usize,
        height: usize,
        index: usize,
    ) -> Option<gdk::MemoryTexture> {
        self.frame_collections.get(&(width, height))?.frame(index)
    }

    pub(crate) fn nearest_frame(
        &self,
        width: usize,
        height: usize,
        index: usize,
    ) -> Option<gdk::MemoryTexture> {
        if let Some(frame) = self.frame(width, height, index) {
            return Some(frame);
        }

        let find_nearest_frame = |collection: &FrameCollection| {
            let mut nearest_index = None;

            for (i, _) in collection
                .lock_frames()
                .iter()
                .enumerate()
                .filter(|(_, t)| t.is_some())
            {
                if i <= index || nearest_index.is_none() {
                    nearest_index = Some(i);
                } else {
                    break;
                }
            }

            nearest_index.map(|index| (index, collection.frame(index).unwrap()))
        };

        let mut current = None;
        let mut current_diff = usize::MAX;

        let mut larger = None;
        let mut larger_diff = usize::MAX;

        let mut smaller = None;
        let mut smaller_diff = usize::MAX;

        if let Some(current_collection) = self.frame_collections.get(&(width, height)) {
            if let Some((i, frame)) = find_nearest_frame(current_collection) {
                current_diff = i.abs_diff(index);
                current = Some(frame);
            }
        }

        if current_diff <= 1 {
            return current;
        }

        for key in self.frame_collections.keys().filter(|(w, _)| *w > width) {
            let collection = self.frame_collections.get(key).unwrap();
            let frame = find_nearest_frame(collection);

            if let Some((i, frame)) = frame {
                if i.abs_diff(index) < larger_diff {
                    larger_diff = i.abs_diff(index);
                    larger = Some(frame)
                }
            }
        }

        for collection in self.frame_collections.values() {
            let frame = find_nearest_frame(collection);

            if let Some((i, frame)) = frame {
                if i.abs_diff(index) < smaller_diff {
                    smaller_diff = i.abs_diff(index);
                    smaller = Some(frame)
                }
            }
        }

        let (min_index, min_diff) = [current_diff, larger_diff, smaller_diff]
            .into_iter()
            .enumerate()
            .min_by_key(|(_, val)| *val)
            .unwrap();

        if current_diff - min_diff <= 1 {
            current
        } else {
            match min_index {
                0 => current,
                1 => larger,
                2 => smaller,
                _ => unreachable!(),
            }
        }
    }

    pub(crate) fn process_requests(&mut self) {
        while let Some(request) = self.requests.pop_front() {
            let mut indexes = vec![];

            if let Some(texture) = self
                .frame_collections
                .get(&request.size)
                .unwrap()
                .frame(request.frame_num)
            {
                request.apply_callback(&texture)
            } else if self.processing.contains(&request) {
                self.processing.push_back(request);
            } else {
                indexes.push(request.index());
                self.processing.push_back(request);
            }

            if !indexes.is_empty() {
                let animation = self.animation.clone();
                let sender = self.sender.clone();
                std::thread::spawn(move || {
                    for index in indexes {
                        let (width, height) = index.size;
                        let size = rlottie::Size::new(width, height);
                        let mut surface = rlottie::Surface::new(size);

                        animation
                            .lock()
                            .unwrap()
                            .render(index.frame_num, &mut surface);

                        let data = glib::Bytes::from_owned(surface);

                        let texture = gdk::MemoryTexture::new(
                            width as i32,
                            height as i32,
                            gdk::MemoryFormat::B8g8r8a8,
                            &data,
                            width * 4,
                        );

                        sender.send((index, texture)).unwrap();
                    }
                });
            }
        }
    }

    pub(crate) fn finish_processed(&mut self) {
        while let Ok((index, texture)) = self.receiver.try_recv() {
            self.frame_collections
                .get(&index.size)
                .unwrap()
                .set_frame(index.frame_num, texture.clone());

            self.processing.retain(|request| {
                if request.index() == index {
                    request.apply_callback(&texture);
                    false
                } else {
                    true
                }
            });
        }
    }

    pub(crate) fn clear_outdated_entries(&mut self) {
        self.frame_collections
            .retain(|_, col| col.seconds_since_last_use() < 10);
    }

    pub(crate) fn tick(&mut self) {
        self.process_requests();
        self.finish_processed();
        self.clear_outdated_entries();
    }

    pub(crate) fn info(&self) -> AnimationInfo {
        self.info
    }
}
