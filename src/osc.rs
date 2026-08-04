use crate::{
    error::{AppError, AppResult},
    model::{OscConfig, ParameterConfig},
};
use rosc::{OscMessage, OscPacket, OscType, encoder};
use std::{
    net::{SocketAddr, ToSocketAddrs, UdpSocket},
    time::Duration,
};

const POINTER_PHASE_OFFSET: i32 = 32;
const PARAMETER_DELAY_MS: u64 = 20;

pub struct OscSender {
    socket: UdpSocket,
    target: SocketAddr,
    pointer_parameter: String,
    character_parameter: String,
    cache: Vec<i32>,
    phases: Vec<bool>,
    parameter_delay: Duration,
}

impl OscSender {
    pub fn new(
        osc: &OscConfig,
        parameters: &ParameterConfig,
        slot_count: usize,
    ) -> AppResult<Self> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;

        let target = resolve_target(&osc.host, osc.port)?;

        Ok(Self {
            socket,
            target,
            pointer_parameter: parameters.pointer.clone(),
            character_parameter: parameters.character.clone(),
            cache: vec![i32::MIN; slot_count],
            phases: vec![false; slot_count],
            parameter_delay: Duration::from_millis(PARAMETER_DELAY_MS),
        })
    }

    pub fn set_target(&mut self, host: &str, port: u16) -> AppResult<()> {
        self.target = resolve_target(host, port)?;

        self.invalidate();

        Ok(())
    }

    pub fn set_parameters(&mut self, parameters: &ParameterConfig) {
        self.pointer_parameter = parameters.pointer.clone();

        self.character_parameter = parameters.character.clone();

        self.invalidate();
    }

    pub fn invalidate(&mut self) {
        self.cache.fill(i32::MIN);
    }

    pub async fn send_changed(&mut self, characters: &[i32]) -> AppResult<usize> {
        self.resize_slots(characters.len());

        let mut sent = 0;

        for (slot, character) in characters.iter().copied().enumerate() {
            if self.cache[slot] == character {
                continue;
            }

            self.send_slot(slot, character).await?;

            self.cache[slot] = character;
            sent += 1;
        }

        Ok(sent)
    }

    pub async fn force_refresh(&mut self, characters: &[i32]) -> AppResult<usize> {
        self.resize_slots(characters.len());

        self.invalidate();

        self.send_changed(characters).await
    }

    async fn send_slot(&mut self, slot: usize, character: i32) -> AppResult<()> {
        let character_parameter = self.character_parameter.clone();

        let pointer_parameter = self.pointer_parameter.clone();

        self.send_parameter(&character_parameter, character)?;

        tokio::time::sleep(self.parameter_delay).await;

        let phase = self.phases[slot];

        let pointer = slot as i32 + if phase { POINTER_PHASE_OFFSET } else { 0 };

        self.send_parameter(&pointer_parameter, pointer)?;

        self.phases[slot] = !phase;

        tokio::time::sleep(self.parameter_delay).await;

        Ok(())
    }

    fn send_parameter(&self, name: &str, value: i32) -> AppResult<()> {
        let packet = OscPacket::Message(OscMessage {
            addr: format!("/avatar/parameters/{name}"),
            args: vec![OscType::Int(value)],
        });

        let bytes =
            encoder::encode(&packet).map_err(|error| AppError::Message(error.to_string()))?;

        self.socket.send_to(&bytes, self.target)?;

        Ok(())
    }

    fn resize_slots(&mut self, slot_count: usize) {
        if self.cache.len() != slot_count {
            self.cache = vec![i32::MIN; slot_count];

            self.phases = vec![false; slot_count];
        }
    }
}

fn resolve_target(host: &str, port: u16) -> AppResult<SocketAddr> {
    (host, port)
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| AppError::Message(format!("could not resolve OSC target {host}:{port}")))
}
