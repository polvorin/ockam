use core::fmt;
use ockam_core::compat::collections::HashMap;

// Open questions:
//
//     1. Should uur's also be the actor address?
//        - Not in Ockam. But they _should_ uniquely designate the actor at
//          a given address.
//     2. Do workers only ever have one uur endpoint?
//     3. Do we re-use a uur if it has been revoked and then granted again?

/// UniqueUnforgeablereference
#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize)]
pub struct UniqueUnforgeableReference(pub u64);

impl fmt::Display for UniqueUnforgeableReference {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        println!("Converting {:?} -> {}", self, self.0);
        fmt.write_str(&format!("{:#08X}", self.0))
    }
}

impl PartialEq for UniqueUnforgeableReference {
    fn eq(&self, rhs: &UniqueUnforgeableReference) -> bool {
        self.0 == rhs.0
    }
}

/// Capability
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct Capability {
    /// The unique unforgeable reference that represents this capability
    pub uur: UniqueUnforgeableReference,
    /// A human-friendly name for this capability
    pub name: String,
    // TODO expires: DateTime
}

/// Capabilities
pub type Capabilities = HashMap<&'static str, Capability>;
