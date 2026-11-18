//! 摇杆输入法的核心：输入层、按键功能，以及把摇杆操作变成文字的状态机。

pub mod layout;
pub mod machine;
pub mod userdict;

pub use layout::{Action, Dir, Held, Layer, DEFAULT_LAYERS};
pub use machine::{warm_up, Caps, Effect, Key, Machine, View};
pub use userdict::UserDict;
