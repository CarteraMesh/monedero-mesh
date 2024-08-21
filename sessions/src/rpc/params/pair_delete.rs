//! https://specs.walletconnect.com/2.0/specs/clients/core/pairing/rpc-methods

use super::IrnMetadata;
use serde::{Deserialize, Serialize};

pub(super) const IRN_REQUEST_METADATA: IrnMetadata = IrnMetadata {
    tag: crate::rpc::TAG_PAIR_DELETE_REQUEST,
    ttl: 30, // 86400 https://specs.walletconnect.com/2.0/specs/clients/core/pairing/rpc-methods#wc_pairingdelete
    prompt: false,
};

pub(super) const IRN_RESPONSE_METADATA: IrnMetadata = IrnMetadata {
    tag: crate::rpc::TAG_PAIR_DELETE_RESPONSE,
    ttl: 30, // 86400 https://specs.walletconnect.com/2.0/specs/clients/core/pairing/rpc-methods#wc_pairingdelete
    prompt: false,
};

#[derive(Debug, Serialize, PartialEq, Eq, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PairDeleteRequest {}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::tests::param_serde_test;
    use anyhow::Result;

    #[test]
    fn test_serde_pair_delete_request() -> Result<()> {
        let json = r#"{}"#;

        param_serde_test::<PairDeleteRequest>(json)
    }
}
