#[derive(Debug, Clone, Copy)]
pub(crate) struct AnimationInfo {
    pub(crate) totalframe: usize,
    pub(crate) default_size: (i32, i32),
    pub(crate) frame_delay: std::time::Duration,
}
