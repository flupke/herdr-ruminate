#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum TunnelState {
    #[default]
    Off,
    Opening,
    Open {
        url: String,
    },
    Failed(String),
}

impl TunnelState {
    pub fn is_on(&self) -> bool {
        matches!(self, Self::Opening | Self::Open { .. })
    }
}
