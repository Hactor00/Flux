use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum Rule {
    Disabled,
    Ratio { ratio_milli: u32 },
    Duration { minutes: u32 },
    Either { ratio_milli: u32, minutes: u32 },
    Infinite,
}
impl Default for Rule {
    fn default() -> Self {
        Self::Ratio { ratio_milli: 1000 }
    }
}
impl Rule {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Ratio { ratio_milli } | Self::Either { ratio_milli, .. } => ensure!(
                (1..=500_000).contains(ratio_milli),
                "Seeding ratio must be between 0.001 and 500"
            ),
            _ => {}
        }
        match self {
            Self::Duration { minutes } | Self::Either { minutes, .. } => ensure!(
                (1..=525_600).contains(minutes),
                "Seeding duration must be between one minute and one year"
            ),
            _ => {}
        }
        Ok(())
    }
    pub(crate) fn reached(&self, uploaded: u64, downloaded: u64, seconds: u64) -> bool {
        let ratio =
            |milli: u32| u128::from(uploaded) * 1000 >= u128::from(downloaded) * u128::from(milli);
        match *self {
            Self::Disabled => true,
            Self::Infinite => false,
            Self::Ratio { ratio_milli } => ratio(ratio_milli),
            Self::Duration { minutes } => seconds >= u64::from(minutes) * 60,
            Self::Either {
                ratio_milli,
                minutes,
            } => ratio(ratio_milli) || seconds >= u64::from(minutes) * 60,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ratios_use_verified_payload_and_do_not_overflow() {
        let rule = Rule::Ratio { ratio_milli: 1500 };
        assert!(!rule.reached(149, 100, 0));
        assert!(rule.reached(150, 100, 0));
        assert!(!rule.reached(u64::MAX, u64::MAX, 0));
        assert!(Rule::Duration { minutes: 1 }.reached(0, 100, 60));
        assert!(!Rule::Duration { minutes: 1 }.reached(0, 100, 59));
        assert!(!Rule::Infinite.reached(u64::MAX, 0, u64::MAX));
    }
}
