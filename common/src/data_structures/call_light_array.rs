use driver_rust::elevio::elev::MotorDirection::{Down, Up};
use crate::config::N_FLOOR;
use crate::constants::N_BUTTONS;
use crate::data_structures::call_request::CallRequest;
use crate::data_structures::call_request::CallRequest::{Cab, Hall};

pub(super) const RAW_CALL_LIGHT_ARRAY_SIZE: usize = N_FLOOR as usize * N_BUTTONS;

/// Utility struct used to represent the call buttons lights.
/// This provides a quick way to convert to raw bytes or to usable, driver-ready array
#[derive(Debug, Copy, Clone)]
pub struct CallLightArray {
    light_array: [[bool; N_BUTTONS]; N_FLOOR as usize]
}

impl CallLightArray {
    /// Convert the [CallLightArray] into a raw bytes array to use in network related code.
    pub(super) fn encode(&self) -> [u8; RAW_CALL_LIGHT_ARRAY_SIZE] {
        self.light_array
            .iter()
            .flatten()
            .map(|x| *x as u8)
            .collect::<Vec<u8>>()
            .try_into()
            .unwrap()
    }

    /// Inverse function of [encode](CallLightArray::encode)
    pub(super) fn decode(raw_bytes: &[u8]) -> Self {
        debug_assert_eq!(raw_bytes.len(), RAW_CALL_LIGHT_ARRAY_SIZE);
        let mut light_array = [[false; N_BUTTONS]; N_FLOOR as usize];
        let raw_lights = raw_bytes
            .iter()
            .map(|x| *x != 0)
            .collect::<Vec<bool>>();

        for (idx, floor_light_array) in light_array.iter_mut().enumerate() {
            floor_light_array.copy_from_slice(
                &raw_lights[N_BUTTONS * idx..N_BUTTONS * idx + N_BUTTONS]
            )
        }

        Self {
            light_array
        }
    }

    /// Generate a [CallLightArray] from a structured but raw representation of the call buttons lights
    pub fn from(light_array: [[bool; N_BUTTONS]; N_FLOOR as usize]) -> Self {
        Self {
            light_array,
        }
    }

    /// Convert a [CallLightArray] into a driver-ready representation of the call buttons lights.
    /// This is used to avoid errors while interacting with lights control.
    pub fn into_usable_light_control(self) -> Vec<(CallRequest, bool)> {
        self.light_array
            .iter()
            .enumerate()
            .fold(vec![], |mut acc, (floor, floor_light_array)| {
                let floor = floor as u8;
                acc.push((Hall { floor, direction: Up }, floor_light_array[0]));
                acc.push((Hall { floor, direction: Down }, floor_light_array[1]));
                acc.push((Cab { floor }, floor_light_array[2]));
                acc
            })
    }
}