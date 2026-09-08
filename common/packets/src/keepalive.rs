use macros::packet;

pub const PING_PACKET: Ping = Ping {};
pub const PONG_PACKET: Pong = Pong {};

#[packet(id = 0x04, clientbound)]
pub struct Ping;

#[packet(id = 0x05, serverbound)]
pub struct Pong;