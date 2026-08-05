use crate::{
    error::{AppError, AppResult},
    model::{OscConfig, ParameterConfig},
};

use rosc::{OscMessage, OscPacket, OscType, encoder};

use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};

const POINTER_PHASE_OFFSET: i32 = 32;
const POINTER_PRIME_VALUE: i32 = 63;
const MAX_SLOT_COUNT: usize = 31;

#[derive(Clone, Copy, Debug)]
enum WritePhase {
    PrimePointer,

    Ready,

    CharacterLatched { slot: usize, character: i32 },
}

#[derive(Clone, Copy, Debug)]
pub enum WriteEvent {
    Idle,

    PointerPrimed,

    CharacterSent { slot: usize, character: i32 },

    SlotCommitted { slot: usize, character: i32 },

    FrameCompleted { slot: usize, character: i32 },
}

pub struct OscSender {
    socket: UdpSocket,
    target: SocketAddr,

    pointer_parameter: String,
    character_parameter: String,

    committed: Vec<i32>,
    target_frame: Vec<i32>,

    pending_slots: Vec<usize>,
    pending_index: usize,

    phase: WritePhase,
    last_pointer: Option<i32>,
}

impl OscSender {
    pub fn new(
        osc: &OscConfig,
        parameters: &ParameterConfig,
        slot_count: usize,
    ) -> AppResult<Self> {
        validate_slot_count(slot_count)?;

        let target = resolve_target(&osc.host, osc.port)?;

        Ok(Self {
            socket: bind_socket(target)?,

            target,

            pointer_parameter: parameters.pointer.clone(),

            character_parameter: parameters.character.clone(),

            committed: vec![i32::MIN; slot_count],

            target_frame: vec![i32::MIN; slot_count],

            pending_slots: Vec::new(),

            pending_index: 0,

            phase: WritePhase::PrimePointer,

            last_pointer: None,
        })
    }

    pub fn set_target(&mut self, host: &str, port: u16) -> AppResult<()> {
        let target = resolve_target(host, port)?;

        if target.is_ipv4() != self.target.is_ipv4() {
            self.socket = bind_socket(target)?;
        }

        self.target = target;

        self.reset_sync(self.committed.len())?;

        Ok(())
    }

    pub fn set_parameters(&mut self, parameters: &ParameterConfig) -> AppResult<()> {
        if self.pointer_parameter == parameters.pointer
            && self.character_parameter == parameters.character
        {
            return Ok(());
        }

        self.pointer_parameter = parameters.pointer.clone();

        self.character_parameter = parameters.character.clone();

        self.reset_sync(self.committed.len())
    }

    pub fn reset_sync(&mut self, slot_count: usize) -> AppResult<()> {
        validate_slot_count(slot_count)?;

        self.committed = vec![i32::MIN; slot_count];

        self.target_frame = vec![i32::MIN; slot_count];

        self.pending_slots.clear();
        self.pending_index = 0;

        self.phase = WritePhase::PrimePointer;

        self.last_pointer = None;

        Ok(())
    }

    pub fn is_idle(&self) -> bool {
        matches!(self.phase, WritePhase::Ready) && self.pending_index >= self.pending_slots.len()
    }

    pub fn begin_frame(&mut self, frame: &[i32]) -> AppResult<bool> {
        validate_slot_count(frame.len())?;

        if !self.is_idle() {
            return Ok(false);
        }

        if frame.len() != self.committed.len() {
            self.reset_sync(frame.len())?;

            return Ok(false);
        }

        self.target_frame.copy_from_slice(frame);

        self.pending_slots.clear();
        self.pending_index = 0;

        self.pending_slots.extend(
            self.target_frame
                .iter()
                .zip(self.committed.iter())
                .enumerate()
                .filter_map(|(slot, (target, committed))| {
                    if target != committed {
                        Some(slot)
                    } else {
                        None
                    }
                }),
        );

        Ok(!self.pending_slots.is_empty())
    }

    pub fn pending_count(&self) -> usize {
        self.pending_slots.len().saturating_sub(self.pending_index)
    }

    pub fn tick(&mut self) -> AppResult<WriteEvent> {
        match self.phase {
            WritePhase::PrimePointer => {
                self.send_parameter(&self.pointer_parameter, POINTER_PRIME_VALUE)?;

                self.last_pointer = Some(POINTER_PRIME_VALUE);

                self.phase = WritePhase::Ready;

                Ok(WriteEvent::PointerPrimed)
            }

            WritePhase::Ready => {
                let Some(&slot) = self.pending_slots.get(self.pending_index) else {
                    return Ok(WriteEvent::Idle);
                };

                let character = self.target_frame[slot];

                self.send_parameter(&self.character_parameter, character)?;

                self.phase = WritePhase::CharacterLatched { slot, character };

                Ok(WriteEvent::CharacterSent { slot, character })
            }

            WritePhase::CharacterLatched { slot, character } => {
                let pointer = self.pointer_value(slot);

                self.send_parameter(&self.pointer_parameter, pointer)?;

                self.last_pointer = Some(pointer);

                self.committed[slot] = character;

                self.pending_index += 1;

                self.phase = WritePhase::Ready;

                if self.pending_index >= self.pending_slots.len() {
                    self.pending_slots.clear();
                    self.pending_index = 0;

                    Ok(WriteEvent::FrameCompleted { slot, character })
                } else {
                    Ok(WriteEvent::SlotCommitted { slot, character })
                }
            }
        }
    }

    fn pointer_value(&self, slot: usize) -> i32 {
        let primary = slot as i32;

        let alternate = primary + POINTER_PHASE_OFFSET;

        if self.last_pointer == Some(primary) {
            alternate
        } else {
            primary
        }
    }

    fn send_parameter(&self, parameter: &str, value: i32) -> AppResult<()> {
        let packet = OscPacket::Message(OscMessage {
            addr: format!("/avatar/parameters/{parameter}"),

            args: vec![OscType::Int(value)],
        });

        let bytes =
            encoder::encode(&packet).map_err(|error| AppError::Message(error.to_string()))?;

        self.socket.send_to(&bytes, self.target)?;

        Ok(())
    }
}

fn validate_slot_count(slot_count: usize) -> AppResult<()> {
    if slot_count == 0 || slot_count > MAX_SLOT_COUNT {
        return Err(AppError::Message(format!(
            "The pointer protocol supports 1 to \
                     {MAX_SLOT_COUNT} slots, but \
                     {slot_count} were requested."
        )));
    }

    Ok(())
}

fn bind_socket(target: SocketAddr) -> AppResult<UdpSocket> {
    if target.is_ipv6() {
        Ok(UdpSocket::bind("[::]:0")?)
    } else {
        Ok(UdpSocket::bind("0.0.0.0:0")?)
    }
}

fn resolve_target(host: &str, port: u16) -> AppResult<SocketAddr> {
    let addresses = (host, port).to_socket_addrs()?.collect::<Vec<_>>();

    addresses
        .iter()
        .copied()
        .find(SocketAddr::is_ipv4)
        .or_else(|| addresses.first().copied())
        .ok_or_else(|| {
            AppError::Message(format!(
                "Could not resolve OSC target \
                         {host}:{port}."
            ))
        })
}
