//! https://specs.walletconnect.com/2.0/specs/clients/sign/rpc-methods
//! #wc_sessiondelete

use serde::{Deserialize, Serialize};

use super::IrnMetadata;

pub(super) const IRN_REQUEST_METADATA: IrnMetadata = IrnMetadata {
    tag: 1112,
    ttl: 86400,
    prompt: false,
};

pub(super) const IRN_RESPONSE_METADATA: IrnMetadata = IrnMetadata {
    tag: 1113,
    ttl: 86400,
    prompt: false,
};

#[derive(Debug, Serialize, PartialEq, Eq, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SessionDeleteRequest {
    pub code: i64,
    pub message: String,
}

impl Default for SessionDeleteRequest {
    fn default() -> Self {
        Self {
            code: crate::rpc::sdkerrors::USER_DISCONNECTED.code,
            message: String::from(crate::rpc::sdkerrors::USER_DISCONNECTED.message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::tests::param_serde_test;
    use anyhow::Result;

    #[test]
    fn test_serde_session_delete_request() -> Result<()> {
        let json = r#"
        {
            "code": 1675757972688031,
            "message": "some message"
        }
        "#;

        param_serde_test::<SessionDeleteRequest>(json)
    }
}
