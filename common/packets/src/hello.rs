
use macros::{PacketField, packet};

#[derive(Debug, PacketField, Clone, PartialEq, Eq)]
pub enum HelloOperationMode {
    Doctor = 0,
    Receptionist = 1,
}

#[packet(id = 0x00, clientbound)]
pub struct Hello {
    pub computer_name: String,
    pub mode: HelloOperationMode,
}
