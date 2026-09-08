use macros::packet;

#[packet(id = 0x02, clientbound)]
pub struct AuthChallenge {
    pub challenge_string: String,
    pub available_users: Vec<(String, String)>, // (username, display_name)
}

#[packet(id = 0x03, serverbound)]
pub struct AuthResponse {
    pub username: String,
    pub password_hash: Vec<u8>,
}

#[packet(id = 0x06, clientbound)] // New packet
pub struct AuthStatus {
    pub success: bool,
    pub roles: Vec<String>,
}

#[packet(id = 0x25, serverbound)]
pub struct RequestSTTAPIKeyPacket;


#[packet(id = 0x26, serverbound)]
pub struct STTAPIKeyResponsePacket {
    pub key: String
}

