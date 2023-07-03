use gtk::gdk;

type FrameCallback = Box<dyn Fn(&gdk::MemoryTexture) + Send>;

pub(crate) struct FrameRequest {
    pub(crate) size: (usize, usize),
    pub(crate) frame_num: usize,
    pub(crate) callback: Option<FrameCallback>,
}

impl std::fmt::Debug for FrameRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrameRequest")
            .field("size", &self.size)
            .field("frame_num", &self.frame_num)
            .finish()
    }
}

impl PartialEq for FrameRequest {
    fn eq(&self, other: &Self) -> bool {
        self.size == other.size && self.frame_num == other.frame_num
    }
}

impl Eq for FrameRequest {}

impl FrameRequest {
    pub(crate) fn index(&self) -> FrameRequestIndex {
        FrameRequestIndex {
            size: self.size,
            frame_num: self.frame_num,
        }
    }

    pub(crate) fn apply_callback(&self, texture: &gdk::MemoryTexture) {
        if let Some(callback) = &self.callback {
            (*callback)(texture)
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct FrameRequestIndex {
    pub(crate) size: (usize, usize),
    pub(crate) frame_num: usize,
}
