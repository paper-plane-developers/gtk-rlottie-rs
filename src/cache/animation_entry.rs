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

    pub(crate) fn request_frame<F>(
        &mut self,
        width: usize,
        height: usize,
        index: usize,
        callback: F,
    ) where
        F: Fn(&gdk::MemoryTexture) + 'static + Send,
    {
        let collection = self
            .frame_collections
            .entry((width, height))
            .or_insert_with(|| FrameCollection::new(self.info.totalframe));

        if let Some(frame) = collection.frame(index) {
            callback(&frame);
        } else {
            let request = FrameRequest::new(width, height, index, callback);
            self.requests.push_back(request);
        }
    }

    pub(crate) fn frame_immediate(
        &self,
        width: usize,
        height: usize,
        index: usize,
    ) -> Option<gdk::MemoryTexture> {
        self.frame_collections.get(&(width, height))?.frame(index)
    }

    pub(crate) fn nearest_frame_immediate(
        &self,
        width: usize,
        height: usize,
        index: usize,
    ) -> Option<gdk::MemoryTexture> {
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

            if let Some(index) = nearest_index {
                collection.frame(index)
            } else {
                None
            }
        };

        if let Some(current_collection) = self.frame_collections.get(&(width, height)) {
            let frame = find_nearest_frame(current_collection);
            if frame.is_some() {
                return frame;
            }
        }

        for key in self.frame_collections.keys().filter(|(w, _)| *w > width) {
            let collection = self.frame_collections.get(key).unwrap();
            let frame = find_nearest_frame(collection);
            if frame.is_some() {
                return frame;
            }
        }

        for collection in self.frame_collections.values() {
            let frame = find_nearest_frame(collection);
            if frame.is_some() {
                return frame;
            }
        }

        None
    }

    pub(crate) fn process_requests(&mut self) {
        while let Some(request) = self.requests.pop_front() {
            if let Some(frame) = self
                .frame_collections
                .get(&request.size)
                .unwrap()
                .frame(request.frame_num)
            {
                (*request.callback)(&frame);
            } else if self.processing.contains(&request) {
                self.processing.push_back(request);
            } else {
                let index = request.index();
                self.processing.push_back(request);
                let animation = self.animation.clone();
                let sender = self.sender.clone();
                std::thread::spawn(move || {
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
                });
            }
        }
    }

    pub(crate) fn finish_processed(&mut self) {
        let mut frames = vec![];

        while let Ok(frame) = self.receiver.try_recv() {
            frames.push(frame);
        }

        if frames.len() > 1 {
            // If we can have many frames in queue
            dbg!(frames.len());
        }

        for (index, texture) in frames {
            self.frame_collections
                .get(&index.size)
                .unwrap()
                .set_frame(index.frame_num, texture.clone());

            self.processing.retain(|request| {
                if request.index() == index {
                    (*request.callback)(&texture);
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
