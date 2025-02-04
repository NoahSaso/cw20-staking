use crate::msg::LockInfo;
use cosmwasm_schema::cw_serde;
use cw_storage_plus::{SnapshotMap, Strategy};
use oraiswap::{asset::AssetRaw, querier::calc_range_start};

use cosmwasm_std::{
    Addr, CanonicalAddr, Decimal, Order, StdError, StdResult, Storage, Timestamp, Uint128,
};
use cosmwasm_storage::{singleton, singleton_read, Bucket, ReadonlyBucket};

pub static KEY_CONFIG: &[u8] = b"config_v2";
pub static PREFIX_POOL_INFO: &[u8] = b"pool_info_v3";
pub static PREFIX_REWARD: &[u8] = b"reward_v3";
pub static PREFIX_STAKER: &[u8] = b"staker_v3";
pub static PREFIX_REWARDS_PER_SEC: &[u8] = b"rewards_per_sec_v3";
// Unbonded
pub static UNBONDING_PERIOD: &[u8] = b"unbonding_period";
pub static LOCK_INFO: &[u8] = b"locking_users";

pub const DEFAULT_LIMIT: u32 = 10;
pub const MAX_LIMIT: u32 = 30;

#[cw_serde]
pub struct Config {
    pub owner: CanonicalAddr,
    pub rewarder: CanonicalAddr,
}

pub fn store_config(storage: &mut dyn Storage, config: &Config) -> StdResult<()> {
    singleton(storage, KEY_CONFIG).save(config)
}

pub fn read_config(storage: &dyn Storage) -> StdResult<Config> {
    singleton_read(storage, KEY_CONFIG).load()
}

#[cw_serde]
pub struct PoolInfo {
    pub staking_token: CanonicalAddr,
    pub pending_reward: Uint128, // not distributed amount due to zero bonding
    pub total_bond_amount: Uint128,
    pub reward_index: Decimal,
}

pub fn store_pool_info(
    storage: &mut dyn Storage,
    asset_key: &[u8],
    pool_info: &PoolInfo,
) -> StdResult<()> {
    Bucket::new(storage, PREFIX_POOL_INFO).save(asset_key, pool_info)
}

pub fn read_pool_info(storage: &dyn Storage, asset_key: &[u8]) -> StdResult<PoolInfo> {
    ReadonlyBucket::new(storage, PREFIX_POOL_INFO).load(asset_key)
}

pub fn read_all_pool_infos(storage: &dyn Storage) -> StdResult<Vec<(Vec<u8>, PoolInfo)>> {
    ReadonlyBucket::<PoolInfo>::new(storage, PREFIX_POOL_INFO)
        .range(None, None, cosmwasm_std::Order::Ascending)
        .collect()
}

#[cw_serde]
pub struct RewardInfo {
    pub native_token: bool,
    pub index: Decimal,
    pub bond_amount: Uint128,
    pub pending_reward: Uint128,
    // this is updated by the owner of this contract, when changing the reward_per_sec
    pub pending_withdraw: Vec<AssetRaw>,
}

/// returns a bucket with all rewards owned by this staker (query it by staker)
pub fn rewards_store<'a>(storage: &'a mut dyn Storage, staker: &[u8]) -> Bucket<'a, RewardInfo> {
    Bucket::multilevel(storage, &[PREFIX_REWARD, staker])
}

/// returns a bucket with all rewards owned by this staker (query it by staker)
/// (read-only version for queries)
pub fn rewards_read<'a>(storage: &'a dyn Storage, staker: &[u8]) -> ReadonlyBucket<'a, RewardInfo> {
    ReadonlyBucket::multilevel(storage, &[PREFIX_REWARD, staker])
}

/// returns a bucket with all stakers belong by this staker (query it by staker)
pub fn stakers_store<'a>(storage: &'a mut dyn Storage, asset_key: &[u8]) -> Bucket<'a, bool> {
    Bucket::multilevel(storage, &[PREFIX_STAKER, asset_key])
}

/// returns a bucket with all rewards owned by this staker (query it by staker)
/// (read-only version for queries)
pub fn stakers_read<'a>(storage: &'a dyn Storage, asset_key: &[u8]) -> ReadonlyBucket<'a, bool> {
    ReadonlyBucket::multilevel(storage, &[PREFIX_STAKER, asset_key])
}

pub fn store_rewards_per_sec(
    storage: &mut dyn Storage,
    asset_key: &[u8],
    assets: Vec<AssetRaw>,
) -> StdResult<()> {
    let mut weight_bucket: Bucket<Vec<AssetRaw>> = Bucket::new(storage, PREFIX_REWARDS_PER_SEC);
    weight_bucket.save(asset_key, &assets)
}

pub fn read_rewards_per_sec(storage: &dyn Storage, asset_key: &[u8]) -> StdResult<Vec<AssetRaw>> {
    let weight_bucket: ReadonlyBucket<Vec<AssetRaw>> =
        ReadonlyBucket::new(storage, PREFIX_REWARDS_PER_SEC);
    weight_bucket.load(asset_key)
}

pub fn store_unbonding_period(
    storage: &mut dyn Storage,
    asset_key: &[u8],
    period: u64,
) -> StdResult<()> {
    Bucket::new(storage, UNBONDING_PERIOD).save(asset_key, &period)
}

pub fn read_unbonding_period(storage: &dyn Storage, asset_key: &[u8]) -> StdResult<u64> {
    ReadonlyBucket::new(storage, UNBONDING_PERIOD).load(asset_key)
}

pub fn insert_lock_info(
    storage: &mut dyn Storage,
    asset_key: &[u8],
    user: &[u8],
    lock_info: LockInfo,
) -> StdResult<()> {
    Bucket::multilevel(storage, &[LOCK_INFO, asset_key, user]).save(
        &lock_info.unlock_time.seconds().to_be_bytes(),
        &lock_info.amount,
    )
}

pub fn read_user_lock_info(
    storage: &dyn Storage,
    asset_key: &[u8],
    user: &[u8],
    start_after: Option<u64>,
    limit: Option<u32>,
    order: Option<i32>,
) -> StdResult<Vec<LockInfo>> {
    let order_by = Order::try_from(order.unwrap_or(1))?;

    let start_after = start_after.map(|a| a.to_be_bytes().to_vec());

    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;

    let (start, end) = match order_by {
        Order::Ascending => (calc_range_start(start_after), None),
        Order::Descending => (None, start_after),
    };

    ReadonlyBucket::multilevel(storage, &[LOCK_INFO, asset_key, user])
        .range(start.as_deref(), end.as_deref(), order_by)
        .take(limit)
        .map(|item| {
            let (time, amount) = item?;
            Ok(LockInfo {
                unlock_time: Timestamp::from_seconds(u64::from_be_bytes(
                    time.try_into()
                        .map_err(|_| StdError::generic_err("Casting u64 to timestamp fail"))?,
                )),
                amount,
            })
        })
        .collect()
}

pub fn remove_and_accumulate_lock_info(
    storage: &mut dyn Storage,
    asset_key: &[u8],
    user: &[u8],
    timestamp: Timestamp,
) -> StdResult<Uint128> {
    let mut bucket = Bucket::<Uint128>::multilevel(storage, &[LOCK_INFO, asset_key, user]);
    let mut remove_timestamps = vec![];
    let mut accumulate_amount = Uint128::zero();

    // use temporay cursor
    {
        let mut cursor = bucket.range(None, None, Order::Ascending);
        let time_in_seconds = timestamp.seconds().to_be_bytes().to_vec();
        while let Some(Ok((time, amount))) = cursor.next() {
            if time.cmp(&time_in_seconds) == std::cmp::Ordering::Greater {
                break;
            }
            remove_timestamps.push(time);
            accumulate_amount += amount;
        }
    }

    // remove timestamp
    for time in remove_timestamps {
        bucket.remove(&time);
    }

    Ok(accumulate_amount)
}

pub const STAKED_BALANCES: SnapshotMap<(&[u8], &Addr), Uint128> = SnapshotMap::new(
    "staked_balances",
    "staked_balance__checkpoints",
    "staked_balance__changelog",
    Strategy::EveryBlock,
);

pub const STAKED_TOTAL: SnapshotMap<&[u8], Uint128> = SnapshotMap::new(
    "total_staked",
    "total_staked__checkpoints",
    "total_staked__changelog",
    Strategy::EveryBlock,
);
