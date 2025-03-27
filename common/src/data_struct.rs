use crate::config::{CLIENT_COUNT, N_FLOOR, VALID_CLIENT_IDS};
use crate::connection::event_handle::handle_state::ConnectionIdentifier;
use crate::data_struct::CabinState::{Between, DoorOpen, Idle, Init};
use crate::data_struct::CallRequest::{Cab, Hall};
use crate::data_struct::ControllerState::{Backup, Master, MasterSteppingDown};
use driver_rust::elevio::elev::MotorDirection::Down;
use driver_rust::elevio::elev::{CallType, MotorDirection};
use std::cmp::Ordering;
use MotorDirection::Up;

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum CallRequest {
    Hall { floor: u8, direction: MotorDirection },
    Cab { floor: u8 }
}

impl Into<CallType> for CallRequest {
    fn into(self) -> CallType {
        match self {
            Cab { .. } => CallType::Cab,
            Hall { direction, .. } => match direction {
                MotorDirection::Down => CallType::HallDown,
                Up => CallType::HallUp,
                MotorDirection::Stop => unreachable!()
            }
        }
    }
}

impl CallRequest {
    pub(crate) fn encode(&self) -> [u8; 3] {
        let mut message = [0u8; 3];
        match self {
            Hall { floor, direction } => {
                message[0] = 0;
                message[1] = *floor;
                message[2] = *direction as u8;
            }
            Cab { floor } => {
                message[0] = 1;
                message[1] = *floor;

            }
        };
        message
    }

    pub(super) fn decode(raw_button: &[u8]) -> Self {
        assert_eq!(raw_button.len(), 3);
        match raw_button[0] {
            0 => Hall { floor: raw_button[1], direction: raw_button[2].try_into().unwrap() },
            _ => Cab { floor: raw_button[1] }
        }
    }

    /// This method returns the 'target' of the request.
    ///
    /// A `target` is the floor the cabin needs to reach to complete the call
    ///
    /// A Hall button `target` is the floor the button sits at.
    ///
    /// A Cab button `target` is the value associated to the button.
    pub fn target(&self) -> u8 {
        match self { Hall { floor, .. } | Cab { floor } => *floor }
    }

    /// This method return the `direction` of the request, if it has one.
    ///
    /// Only [Hall](Request::Hall) requests have a `direction`.
    pub fn direction(&self) -> Option<MotorDirection> {
        match *self {
            Cab {..} => None,
            Hall { direction, .. } => Some(direction)
        }
    }

    /// This method returns the `light_id` associated with the request.
    ///
    /// A `light_id` is used to turn of and on specific call light on the elevator control panel.
    pub fn light_id(&self) -> u8 {
        match *self {
            Cab { .. } => CallType::Cab as u8,
            Hall { direction, .. } => match direction {
                Up => CallType::HallUp as u8,
                MotorDirection::Down => CallType::HallDown as u8,
                _ => unreachable!("Shouldn't happen")
            }
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum CabinState {
    Init,
    Idle { current_floor: u8 },
    Between { from_floor: u8, to_floor: u8 }, // Moving between to floors
    DoorOpen { current_floor: u8 },
}

impl Default for CabinState { // is this used?
    fn default() -> Self {
        Init
    }
}

impl CabinState {
    pub fn is_between(&self) -> bool {
        if let Between { .. } = *self {
            true
        } else { false }
    }

    pub fn is_door_open(&self) -> bool {
        if let DoorOpen { .. } = *self {
            true
        } else { false }
    }

    pub fn is_idle(&self) -> bool {
        if let Idle { .. } = *self {
            true
        } else { false }
    }

    pub fn is_init(&self) -> bool {
        if let Init = *self {
            true
        } else { false }
    }

    pub fn increment_between(&mut self) {
        let Between { from_floor, to_floor } = *self else { unreachable!() };

        *self = match Self::get_direction_from_to(from_floor, to_floor) {
            MotorDirection::Down => Between { from_floor: to_floor, to_floor: to_floor - 1 },
            Up => Between { from_floor: to_floor, to_floor: to_floor + 1 },
            MotorDirection::Stop => unreachable!()
        }
    }

    pub fn get_direction_relative_to(&self, to: u8) -> MotorDirection {
        match self.get_current_floor_relative_to(to).cmp(&to) {
            Ordering::Less => Up,
            Ordering::Equal => MotorDirection::Stop,
            Ordering::Greater => MotorDirection::Down
        }
    }

    pub fn get_current_floor_relative_to(&self, target: u8) -> u8 {
        match self {
            Idle { current_floor }
            | DoorOpen { current_floor } => *current_floor,
            Between { from_floor, to_floor } => {
                let from_distance = (target as i32 - *from_floor as i32).abs();
                let to_distance = (target as i32 - *to_floor as i32).abs();

                match from_distance.cmp(&to_distance) {
                    Ordering::Less => *to_floor,
                    Ordering::Equal => unreachable!(),
                    Ordering::Greater => *from_floor
                }
            }
            Init => unreachable!("Function \"get_current_floor_relative_to\" should not be called on Init state")
        }
    }

    pub fn get_last_seen_floor(&self) -> u8 {
        match *self {
            DoorOpen { current_floor }
            | Idle { current_floor }
            | Between { from_floor: current_floor, .. } => current_floor,
            Init => unreachable!("Function \"get_last_seen_floor\" should not be called on Init state")
        }
    }

    pub fn get_direction_from_to(from: u8, to: u8) -> MotorDirection {
        match from.cmp(&to) {
            Ordering::Less => Up,
            Ordering::Equal => MotorDirection::Stop,
            Ordering::Greater => MotorDirection::Down
        }
    }

    pub fn get_direction(&self) -> MotorDirection {
        match *self {
            CabinState::Between { from_floor, to_floor } => Self::get_direction_from_to(from_floor, to_floor),
            CabinState::Init => MotorDirection::Down,
            _ => MotorDirection::Stop,

        }
    }

    pub(crate) fn encode(&self) -> [u8; 3] {
        let mut message = [0u8; 3];
        match *self {
            DoorOpen { current_floor } => {
                message[0] = 0;
                message[1] = current_floor;
            }
            Idle { current_floor } => {
                message[0] = 1;
                message[1] = current_floor;
            }
            Between { from_floor, to_floor } => {
                message[0] = 2;
                message[1] = from_floor;
                message[2] = to_floor;
            }
            Init => {
                message[0] = 3;
            }
        };
        message
    }

    pub(super) fn decode(raw_cabin_state: &[u8]) -> Self {
        assert_eq!(raw_cabin_state.len(), 3);
        match raw_cabin_state[0] {
            0 => DoorOpen { current_floor: raw_cabin_state[1] },
            1 => Idle { current_floor: raw_cabin_state[1] },
            2 => Between { from_floor: raw_cabin_state[1], to_floor: raw_cabin_state[2] },
            3 => Init,
            _ => unreachable!()
        }
    }

}

#[derive(Debug, Copy, Clone)]
pub struct CallLightArray {
    light_array: [[bool; 3]; N_FLOOR as usize]
}

impl CallLightArray {
    pub(super) fn encode(&self) -> [u8; (N_FLOOR * 3) as usize] {
        let a = self.light_array
            .iter()
            .flatten()
            .map(|x| *x as u8)
            .collect::<Vec<u8>>()
            .try_into()
            .unwrap();

        println!("{a:?}");
        a
    }

    pub(super) fn decode(raw_bytes: &[u8]) -> Self {
        debug_assert_eq!(raw_bytes.len(), (N_FLOOR * 3) as usize);
        let mut light_array = [[false; 3]; N_FLOOR as usize];
        let raw_lights = raw_bytes
            .iter()
            .map(|x| *x != 0)
            .collect::<Vec<bool>>();

        for (idx, floor_light_array) in light_array.iter_mut().enumerate() {
            floor_light_array.copy_from_slice(
                &raw_lights[3 * idx..3 * idx + 3]
            )
        }


        Self {
            light_array
        }
    }

    pub fn from(light_array: [[bool; 3]; N_FLOOR as usize]) -> Self {
        Self {
            light_array,
        }
    }

    pub fn into_usable_light_control(self) -> Vec<(CallRequest, bool)> {
        self.light_array
            .iter()
            .enumerate()
            .fold(vec![], |mut acc, (floor, floor_light_array)| {
                let floor = floor as u8;
                acc.push((Hall { floor, direction: Up}, floor_light_array[0]));
                acc.push((Hall { floor, direction: Down}, floor_light_array[1]));
                acc.push((Cab { floor }, floor_light_array[2]));
                acc
            })
    }
}

const FLOOR_ARRAY_SIZE: usize = (2 + CLIENT_COUNT) as usize;
const CLIENTS_RAW_SIZE: usize = N_FLOOR as usize * FLOOR_ARRAY_SIZE;
pub(super) const FCRM_RAW_SIZE: usize = CLIENT_COUNT as usize + CLIENTS_RAW_SIZE;

#[derive(Debug, Copy, Clone)]
pub struct FullControllerRequestsMatrix {
    client_ids: [u8; CLIENT_COUNT as usize],
    clients: [[bool; FLOOR_ARRAY_SIZE]; N_FLOOR as usize]
}

impl FullControllerRequestsMatrix {
    pub fn from(
        merged_hall_requests: [[bool; 2]; N_FLOOR as usize],
        clients_cab_requests: [[bool; CLIENT_COUNT as usize]; N_FLOOR as usize]
    ) -> Self {
        let clients = merged_hall_requests.into_iter()
            .zip(clients_cab_requests.into_iter())
            .map(|(floor_hall, floor_cab)| {
                let floor_array_merged: [bool; FLOOR_ARRAY_SIZE] = floor_hall
                    .into_iter()
                    .chain(floor_cab.into_iter())
                    .collect::<Vec<bool>>()
                    .try_into()
                    .unwrap();
                floor_array_merged

            })
            .collect::<Vec<[bool; FLOOR_ARRAY_SIZE]>>()
            .try_into()
            .unwrap();

        Self {
            client_ids: VALID_CLIENT_IDS,
            clients,
        }
    }

    pub(super) fn encode(&self) -> [u8; FCRM_RAW_SIZE] {
        let mut raw_output = [0u8; FCRM_RAW_SIZE];
        raw_output[0..CLIENT_COUNT as usize].copy_from_slice(&self.client_ids);
        let clients: [u8; CLIENTS_RAW_SIZE] = self.clients
            .iter()
            .flatten()
            .map(|x| *x as u8)
            .collect::<Vec<u8>>()
            .try_into()
            .unwrap();
        raw_output[CLIENT_COUNT as usize..FCRM_RAW_SIZE].copy_from_slice(&clients);
        raw_output
    }

    pub(super) fn decode(raw_bytes: &[u8]) -> Self {
        debug_assert_eq!(raw_bytes.len(), FCRM_RAW_SIZE);
        let mut client_ids = [0u8; CLIENT_COUNT as usize];
        client_ids.copy_from_slice(&raw_bytes[0..CLIENT_COUNT as usize]);

        let raw_clients: [bool; CLIENTS_RAW_SIZE] = raw_bytes[CLIENT_COUNT as usize..]
            .iter()
            .map(|x| *x != 0)
            .collect::<Vec<bool>>()
            .try_into()
            .unwrap();

        let mut clients = [[false; FLOOR_ARRAY_SIZE]; N_FLOOR as usize];
        for (floor_index ,floor_matrix) in clients.iter_mut().enumerate() {
            let starting_idx = floor_index * FLOOR_ARRAY_SIZE;
            floor_matrix.copy_from_slice(
                &raw_clients[starting_idx..starting_idx+FLOOR_ARRAY_SIZE]
            )
        }
        
        Self {
            client_ids,
            clients,
        }
    }

    pub fn get_floor_matrix_of(
        &self,
        elevator_id: ConnectionIdentifier,
    ) -> [[bool; 3]; N_FLOOR as usize] {
        let cab_idx = 2 + self.client_ids
            .iter()
            .enumerate()
            .find(|(_, id)| **id == elevator_id )
            .unwrap().0;

        self.clients
            .map(|floor_array| {
                [floor_array[0], floor_array[1], floor_array[cab_idx]]
            })
    }
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum ControllerState {
    /// Currently acting as backup, receiving controller_link message from the current master
    Backup,
    /// Currently a master in the process of being downgraded to a backup, this is the state during reconciliation
    MasterSteppingDown,
    /// Currently a master, handles clients and synchronise every backup.
    Master,
}

impl From<u8> for ControllerState {
    fn from(value: u8) -> Self {
        match value {
            u8::MIN => Master,
            u8::MAX => Backup,
            _ => MasterSteppingDown
        }
    }
}

impl Into<u8> for ControllerState {
    fn into(self) -> u8 {
        match self {
            Backup => u8::MAX,
            MasterSteppingDown => 127,
            Master => u8::MIN
        }
    }
}