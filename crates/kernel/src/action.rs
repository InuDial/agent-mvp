use std::any::Any;

use mvp_contract::Capabilities;

pub trait Action: Any + Send + Sync {
    fn capabilities(&self) -> Capabilities;
}
