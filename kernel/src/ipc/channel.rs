pub trait IpcChannel: Send + Sync {
    fn send(&self, buf: &[u8]) -> Result<(), IpcError>;
    fn recv(&self, buf: &mut [u8]) -> Result<usize, IpcError>;
    fn close(&self);
}

#[derive(Debug)]
pub enum IpcError { Closed, WouldBlock, TooLarge }
