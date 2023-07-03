#[derive(Debug)]
pub(crate) struct UnsafeSend<T>(pub(crate) T);
unsafe impl<T> Send for UnsafeSend<T> {}

impl<T> std::ops::Deref for UnsafeSend<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<T> std::ops::DerefMut for UnsafeSend<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
