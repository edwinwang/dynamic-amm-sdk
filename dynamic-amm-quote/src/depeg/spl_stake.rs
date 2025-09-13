use prog_dynamic_amm::constants::depeg;
use spl_stake_pool::state::StakePool;
use std::convert::TryInto;

pub fn get_virtual_price(bytes: &[u8]) -> Option<u64> {
    // SPL Stake Pool structure is variable size due to ValidatorList
    // We need to extract fields directly from known offsets
    // - total_lamports: u64 (8 bytes) - at offset 258
    // - pool_token_supply: u64 (8 bytes) - at offset 266
    
    if bytes.len() < 274 {
        return None;
    }
    
    // Extract total_lamports and pool_token_supply directly
    let total_lamports = u64::from_le_bytes(
        bytes[258..266].try_into().ok()?
    );
    let pool_token_supply = u64::from_le_bytes(
        bytes[266..274].try_into().ok()?
    );
    
    if pool_token_supply == 0 {
        return None;
    }

    let virtual_price = (total_lamports as u128)
        .checked_mul(depeg::PRECISION as u128)?
        .checked_div(pool_token_supply as u128)?;

    virtual_price.try_into().ok()
}
