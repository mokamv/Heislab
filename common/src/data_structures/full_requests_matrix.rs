use crate::config::{CLIENT_COUNT, N_FLOOR, VALID_CLIENT_IDS};
use crate::connection::event_handle::handle_state::ConnectionIdentifier;
use crate::constants::{N_BUTTONS, N_HALL_BUTTONS};

const FLOOR_ARRAY_SIZE: usize = N_HALL_BUTTONS + CLIENT_COUNT as usize;
const CLIENTS_RAW_SIZE: usize = N_FLOOR as usize * FLOOR_ARRAY_SIZE;
pub(super) const FCRM_RAW_SIZE: usize = CLIENT_COUNT as usize + CLIENTS_RAW_SIZE;

#[derive(Debug, Copy, Clone)]
pub struct FullControllerRequestsMatrix {
    client_ids: [u8; CLIENT_COUNT as usize],
    clients: [[bool; FLOOR_ARRAY_SIZE]; N_FLOOR as usize]
}

impl FullControllerRequestsMatrix {
    pub fn from(
        merged_hall_requests: [[bool; N_HALL_BUTTONS]; N_FLOOR as usize],
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

    pub fn get_requests_matrix_of(
        &self,
        elevator_id: ConnectionIdentifier,
    ) -> [[bool; N_BUTTONS]; N_FLOOR as usize] {
        let cab_idx = N_HALL_BUTTONS + self.client_ids
            .iter()
            .enumerate()
            .find(|(_, id)| **id == elevator_id )
            .unwrap().0;

        self.clients
            .map(|floor_array| {
                [floor_array[0], floor_array[1], floor_array[cab_idx]]
            })
    }

    pub fn get_cab_requests_of(
        &self,
        elevator_id: ConnectionIdentifier
    ) -> [bool; N_FLOOR as usize] {
        let cab_idx = N_HALL_BUTTONS + self.client_ids
            .iter()
            .enumerate()
            .find(|(_, id)| **id == elevator_id )
            .unwrap().0;

        self.clients
            .map(|floor_array| floor_array[cab_idx])
    }
}