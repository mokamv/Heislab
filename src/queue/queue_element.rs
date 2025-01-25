
struct Queueelement {
    request_type: Queuetype,
    floor: Floor,
    previous: Option<Box<Queueelement>>, // Pointer to the previous element in the queue
}

enum Floor {
    First,
    Second,
    Third,
    Fourth,
    Between
}

enum Direction {
    Up,
    Down
}

enum Queuetype {
    Up,
    Down,
    Off
}