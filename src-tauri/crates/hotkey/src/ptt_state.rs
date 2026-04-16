use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PttState {
    Idle,
    Recording,
    Processing,
}

impl Default for PttState {
    fn default() -> Self {
        PttState::Idle
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PttEvent {
    StartedRecording,
    StoppedRecording,
    FinishedProcessing,
    NoChange,
}

#[derive(Debug, Default)]
pub struct PttMachine {
    state: PttState,
}

impl PttMachine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn state(&self) -> PttState {
        self.state
    }

    pub fn on_key_down(&mut self) -> PttEvent {
        match self.state {
            PttState::Idle => {
                self.state = PttState::Recording;
                PttEvent::StartedRecording
            }
            _ => PttEvent::NoChange,
        }
    }

    pub fn on_key_up(&mut self) -> PttEvent {
        match self.state {
            PttState::Recording => {
                self.state = PttState::Processing;
                PttEvent::StoppedRecording
            }
            _ => PttEvent::NoChange,
        }
    }

    pub fn on_finish_processing(&mut self) -> PttEvent {
        match self.state {
            PttState::Processing => {
                self.state = PttState::Idle;
                PttEvent::FinishedProcessing
            }
            _ => PttEvent::NoChange,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_idle() {
        let m = PttMachine::new();
        assert_eq!(m.state(), PttState::Idle);
    }

    #[test]
    fn keydown_from_idle_goes_recording() {
        let mut m = PttMachine::new();
        let ev = m.on_key_down();
        assert_eq!(m.state(), PttState::Recording);
        assert_eq!(ev, PttEvent::StartedRecording);
    }

    #[test]
    fn keyup_from_recording_goes_processing() {
        let mut m = PttMachine::new();
        m.on_key_down();
        let ev = m.on_key_up();
        assert_eq!(m.state(), PttState::Processing);
        assert_eq!(ev, PttEvent::StoppedRecording);
    }

    #[test]
    fn finish_from_processing_goes_idle() {
        let mut m = PttMachine::new();
        m.on_key_down();
        m.on_key_up();
        let ev = m.on_finish_processing();
        assert_eq!(m.state(), PttState::Idle);
        assert_eq!(ev, PttEvent::FinishedProcessing);
    }

    #[test]
    fn keydown_while_recording_is_noop() {
        let mut m = PttMachine::new();
        m.on_key_down();
        let ev = m.on_key_down();
        assert_eq!(m.state(), PttState::Recording);
        assert_eq!(ev, PttEvent::NoChange);
    }

    #[test]
    fn keyup_while_idle_is_noop() {
        let mut m = PttMachine::new();
        let ev = m.on_key_up();
        assert_eq!(m.state(), PttState::Idle);
        assert_eq!(ev, PttEvent::NoChange);
    }

    #[test]
    fn keyup_while_processing_is_noop() {
        let mut m = PttMachine::new();
        m.on_key_down();
        m.on_key_up();
        let ev = m.on_key_up();
        assert_eq!(m.state(), PttState::Processing);
        assert_eq!(ev, PttEvent::NoChange);
    }
}
