use alloc::vec::Vec;

use veles_casper_contract_api::{
    casper_types::{
        EntityAddr, U512,
        bytesrepr::{self, ToBytes},
    },
    veles_casper_contract_macros::CasperMessage,
};

#[derive(Debug, Copy, Clone, PartialEq, Eq, CasperMessage)]
pub struct DidNothing {
    pub caller: EntityAddr,
    pub amount: U512,
}

impl ToBytes for DidNothing {
    fn to_bytes(&self) -> Result<Vec<u8>, bytesrepr::Error> {
        let mut buffer = bytesrepr::allocate_buffer(self)?;
        self.write_bytes(&mut buffer)?;
        Ok(buffer)
    }
    fn serialized_length(&self) -> usize {
        self.caller.serialized_length() + self.amount.serialized_length()
    }
    fn write_bytes(&self, writer: &mut Vec<u8>) -> Result<(), bytesrepr::Error> {
        self.caller.write_bytes(writer)?;
        self.amount.write_bytes(writer)?;
        Ok(())
    }
}
