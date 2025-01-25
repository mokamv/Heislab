
struct Queueelement {
    request_type: Queuetype,
    floor: Floor,
    previous: Option<Box<Queueelement>>, // Pointer to the previous element in the queue
}

enum Position {
    Between,
    Floor(u8)
}

// Impl. match

enum Direction {
    Up,
    Down
}

enum Queuetype {
    Up,
    Down,
    Off
}