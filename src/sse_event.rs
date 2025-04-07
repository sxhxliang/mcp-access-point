

#[derive(Debug, Clone)]
pub struct SseEvent {
    data: String,
    event: Option<String>,
    pub session_id: String
}


impl SseEvent {
    pub fn new_event(session_id:&str, event_type: &str, data: &str) -> Self {
        Self {
            event: Some(event_type.to_string()),
            data: data.to_string(),
            session_id: session_id.to_string()
        }
    }

    pub fn new(session_id :&str, data: &str) -> Self {
        Self {
            data: data.to_string(),
            event: None,
            session_id: session_id.to_string()
        }
    }
    pub fn to_string(&self) -> String {
        let mut output = String::new();
        output.push_str(&format!("event: {}\r\n", self.event.clone().unwrap()));
        output.push_str(&format!("data: {}\r\n\r\n", self.data));
        output
    }
    pub fn to_bytes(&self) -> Vec<u8> {
        if self.event.is_none() {
            self.data.clone().into_bytes()
        } else {
            self.to_string().into_bytes()
        }
    }
}