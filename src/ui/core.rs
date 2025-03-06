struct CliUI {
    message: String,
}

impl CliUI {
    fn new() -> CliUI {
        CliUI {
            message: String::new(),
        }
    }

    fn set_message(&mut self, message: &str) {
        self.message = message.to_string();
    }

    fn show_message(&self) {
        println!("{}", self.message);
    }
    
}