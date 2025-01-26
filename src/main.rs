use exercise_3::single_elevator_controller::event_controller::ElevatorController;

fn main() {
    ElevatorController::begin_loop(
        "localhost:15657", 4
    );
}