pub trait Greeter {
    fn greet(&self) -> String;
}

pub struct English;
impl Greeter for English {
    fn greet(&self) -> String { "hello".into() }
}

pub struct French;
impl Greeter for French {
    fn greet(&self) -> String { "bonjour".into() }
}

pub fn say_english() -> String { English.greet() }
pub fn say_french() -> String { French.greet() }
