#[derive(Debug, PartialEq)]
pub enum SlowState {
    Idle,
    Clarifying,
    PresentingApproaches,
    WaitingForChoice,
    Skeleton,
    Supporting,
    Reviewing,
}

pub enum UserSignal {
    Done,
    Review,
}

pub struct InputDecision {
    pub prefix: &'static str,
}

pub struct FilteredResponse {
    pub display: String,
    pub code_held: bool,
}

pub struct SlowSession {
    pub enabled: bool,
    pub state: SlowState,
    pub held_code: Vec<String>,
}

fn strip_code_blocks(text: &str) -> String {
    use regex::Regex;
    // Named blocks (with filename comment)
    let with_name = Regex::new(r"```(?:\w+)?\s*(?://|#)\s*[^\n]+\n[\s\S]*?```").unwrap();
    // Plain blocks
    let plain = Regex::new(r"```(?:\w+)?\n[\s\S]*?```").unwrap();

    let s = with_name.replace_all(text, "");
    let s = plain.replace_all(&s, "");
    // Collapse runs of blank lines left behind
    let blanks = Regex::new(r"\n{3,}").unwrap();
    blanks.replace_all(&s, "\n\n").trim().to_string()
}

impl SlowSession {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            state: SlowState::Idle,
            held_code: Vec::new(),
        }
    }

    pub fn disable(&mut self) {
        self.enabled = false;
    }

    pub fn process_input(&mut self, _input: &str) -> InputDecision {
        if !self.enabled {
            return InputDecision { prefix: "" };
        }

        self.state = self.next_state();
        InputDecision { prefix: self.system_prefix() }
    }

    fn next_state(&self) -> SlowState {
        match self.state {
            SlowState::Idle                  => SlowState::Clarifying,
            SlowState::Clarifying            => SlowState::PresentingApproaches,
            SlowState::PresentingApproaches  => SlowState::WaitingForChoice,
            SlowState::WaitingForChoice      => SlowState::Skeleton,
            SlowState::Skeleton              => SlowState::Skeleton,
            SlowState::Supporting            => SlowState::Supporting,
            SlowState::Reviewing             => SlowState::Idle,
        }
    }

    pub fn advance(&mut self, signal: UserSignal) {
        if !self.enabled { return; }
        self.state = match signal {
            UserSignal::Done   => SlowState::Supporting,
            UserSignal::Review => SlowState::Reviewing,
        };
    }

    pub fn filter_response(&mut self, raw: &str) -> FilteredResponse {
        if !self.enabled {
            return FilteredResponse { display: raw.to_string(), code_held: false };
        }

        let gate_active = matches!(
            self.state,
            SlowState::Clarifying
                | SlowState::PresentingApproaches
                | SlowState::WaitingForChoice
        );

        if !gate_active {
            return FilteredResponse { display: raw.to_string(), code_held: false };
        }

        let blocks = crate::diff::parse_blocks(raw);
        if blocks.is_empty() {
            return FilteredResponse { display: raw.to_string(), code_held: false };
        }

        for block in blocks {
            self.held_code.push(block.content);
        }

        FilteredResponse {
            display: strip_code_blocks(raw),
            code_held: true,
        }
    }

    fn system_prefix(&self) -> &'static str {
        match self.state {
            SlowState::Clarifying =>
                "SLOW MODE: Ask the user exactly one clarifying question. Do not provide code or a solution yet.",
            SlowState::PresentingApproaches =>
                "SLOW MODE: Present 2 or more distinct approaches with trade-offs. Do not write implementation code.",
            SlowState::WaitingForChoice =>
                "",
            SlowState::Skeleton =>
                "SLOW MODE: Write a code skeleton using comments only. Replace all logic with precise comments describing what to implement. Do not write working code.",
            SlowState::Supporting =>
                "SLOW MODE: The user is implementing. Give hints and ask questions only. Do not provide direct code solutions.",
            SlowState::Reviewing =>
                "SLOW MODE: Review the implementation. Ask questions to verify the user understands every part.",
            SlowState::Idle => "",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_enabled_starts_idle() {
        let s = SlowSession::new(true);
        assert!(s.enabled);
        assert!(matches!(s.state, SlowState::Idle));
        assert!(s.held_code.is_empty());
    }

    #[test]
    fn new_disabled_is_bypass() {
        let s = SlowSession::new(false);
        assert!(!s.enabled);
    }

    #[test]
    fn disable_turns_off_slow() {
        let mut s = SlowSession::new(true);
        s.disable();
        assert!(!s.enabled);
    }

    #[test]
    fn process_input_disabled_returns_empty_prefix() {
        let mut s = SlowSession::new(false);
        let d = s.process_input("implement foo");
        assert_eq!(d.prefix, "");
        assert!(matches!(s.state, SlowState::Idle));
    }

    #[test]
    fn process_input_idle_transitions_to_clarifying() {
        let mut s = SlowSession::new(true);
        let d = s.process_input("implement foo");
        assert!(matches!(s.state, SlowState::Clarifying));
        assert!(!d.prefix.is_empty());
    }

    #[test]
    fn process_input_clarifying_transitions_to_presenting() {
        let mut s = SlowSession::new(true);
        s.state = SlowState::Clarifying;
        s.process_input("it should parse JSON");
        assert!(matches!(s.state, SlowState::PresentingApproaches));
    }

    #[test]
    fn process_input_skeleton_stays_skeleton() {
        let mut s = SlowSession::new(true);
        s.state = SlowState::Skeleton;
        s.process_input("here is my code");
        assert!(matches!(s.state, SlowState::Skeleton));
    }

    #[test]
    fn process_input_reviewing_resets_to_idle() {
        let mut s = SlowSession::new(true);
        s.state = SlowState::Reviewing;
        s.process_input("done reviewing");
        assert!(matches!(s.state, SlowState::Idle));
    }

    #[test]
    fn advance_done_enters_supporting() {
        let mut s = SlowSession::new(true);
        s.state = SlowState::Skeleton;
        s.advance(UserSignal::Done);
        assert!(matches!(s.state, SlowState::Supporting));
    }

    #[test]
    fn advance_review_enters_reviewing() {
        let mut s = SlowSession::new(true);
        s.state = SlowState::Supporting;
        s.advance(UserSignal::Review);
        assert!(matches!(s.state, SlowState::Reviewing));
    }

    #[test]
    fn advance_disabled_is_noop() {
        let mut s = SlowSession::new(false);
        s.advance(UserSignal::Done);
        assert!(matches!(s.state, SlowState::Idle));
    }

    #[test]
    fn filter_disabled_passes_everything_through() {
        let mut s = SlowSession::new(false);
        s.state = SlowState::Clarifying;
        let r = s.filter_response("here is code:\n```rust\nfn foo() {}\n```");
        assert!(!r.code_held);
        assert!(r.display.contains("fn foo()"));
    }

    #[test]
    fn filter_clarifying_strips_code_blocks() {
        let mut s = SlowSession::new(true);
        s.state = SlowState::Clarifying;
        let raw = "Some text\n\n```rust\nfn foo() { 42 }\n```\n\nMore text";
        let r = s.filter_response(raw);
        assert!(r.code_held);
        assert!(!r.display.contains("fn foo()"));
        assert!(r.display.contains("Some text"));
        assert!(r.display.contains("More text"));
        assert_eq!(s.held_code.len(), 1);
    }

    #[test]
    fn filter_presenting_strips_code_blocks() {
        let mut s = SlowSession::new(true);
        s.state = SlowState::PresentingApproaches;
        let raw = "Approach A\n```python\ndef foo(): pass\n```";
        let r = s.filter_response(raw);
        assert!(r.code_held);
        assert!(!r.display.contains("def foo()"));
    }

    #[test]
    fn filter_skeleton_allows_code() {
        let mut s = SlowSession::new(true);
        s.state = SlowState::Skeleton;
        let raw = "Skeleton:\n```rust\n// step 1: do X\n// step 2: do Y\n```";
        let r = s.filter_response(raw);
        assert!(!r.code_held);
        assert!(r.display.contains("step 1"));
    }

    #[test]
    fn filter_no_code_blocks_unchanged() {
        let mut s = SlowSession::new(true);
        s.state = SlowState::Clarifying;
        let raw = "What would you like to build?";
        let r = s.filter_response(raw);
        assert!(!r.code_held);
        assert_eq!(r.display, raw);
    }
}
