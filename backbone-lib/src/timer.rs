//! This module contains a timer, that can generate events for the backend.

struct TimeEntry {
    id: u16,
    remaining_time: f32,
}

pub struct Timer {
    list_of_timers: Vec<TimeEntry>,
}

impl Timer {
    pub fn new() -> Timer {
        Timer {
            list_of_timers: Vec::new(),
        }
    }

    /// Starts a new timer. If a timer with the id already exists it gets overwritten.
    pub fn start_timer(&mut self, id: u16, remaining_time: f32) {
        // We can overwrite an existing timer.
        self.list_of_timers.retain(|e| e.id != id);
        self.list_of_timers.push(TimeEntry { id, remaining_time });
    }

    /// Kills a timer (if still existing).
    pub fn cancel_timer(&mut self, id: u16) {
        self.list_of_timers.retain(|e| e.id != id);
    }

    /// Updates and gets the list with the timers.
    pub fn update_and_get_list(&mut self, delta_time: f32) -> Vec<u16> {
        let mut result = Vec::new();

        for entry in self.list_of_timers.iter_mut() {
            entry.remaining_time -= delta_time;
            if entry.remaining_time <= 0.0 {
                result.push(entry.id);
            }
        }

        self.list_of_timers.retain(|e| !result.contains(&e.id));

        result
    }
}
