# 优化虚拟价格更新方案

## 当前问题

目前 `compute_quote` 函数中会调用 `update_base_virtual_price`，这会：
1. 修改 `pool` 数据（需要 `&mut pool`）
2. 在每次 quote 时都检查缓存过期
3. 可能在 quote 时更新虚拟价格，增加延迟

## 优化方案

将虚拟价格更新移到 `update()` 方法中，让 `compute_quote` 变成纯只读操作。

## 实现步骤

### 1. 修改 meteora.rs 的 update 方法

```rust
fn update(&mut self, account_map: &AccountMap) -> Result<()> {
    // ... 读取所有账户数据 ...
    
    // 在构建缓存数据之前，先更新虚拟价格
    let mut pool_updated = pool.clone();
    
    // 构建 Clock
    let clock = Clock {
        slot: self.clock_ref.slot.load(Ordering::Relaxed),
        epoch_start_timestamp: 0,
        epoch: 0,
        leader_schedule_epoch: 0,
        unix_timestamp: self.clock_ref.unix_timestamp.load(Ordering::Relaxed),
    };
    
    // 更新虚拟价格（如果需要）
    if let CurveType::Stable { depeg, .. } = &mut pool_updated.curve_type {
        if !depeg.depeg_type.is_none() {
            // 注意：update_base_virtual_price 会修改 pool
            update_base_virtual_price(&mut pool_updated, &clock, stake_data.clone())?;
            
            info!("[METEORA] Updated virtual price for pool {}: base_virtual_price={}, cache_updated={}", 
                self.config.pool_address, 
                depeg.base_virtual_price,
                depeg.base_cache_updated
            );
        }
    }
    
    // 构建缓存数据（使用更新后的 pool）
    let cached_data = CachedQuoteData {
        pool: pool_updated,  // 使用更新后的 pool
        // ... 其他字段 ...
    };
    
    *self.cached_data.write() = Some(cached_data);
}
```

### 2. 修改 SDK 的 compute_quote 函数

创建一个新版本的 `compute_quote`，不修改 pool：

```rust
// 新增一个不修改 pool 的版本
pub fn compute_quote_readonly(
    in_token_mint: Pubkey,
    in_amount: u64,
    quote_data: &QuoteData,  // 注意：这里用引用，不获取所有权
) -> anyhow::Result<QuoteResult> {
    // 检查虚拟价格是否已更新
    if let CurveType::Stable { depeg, .. } = &quote_data.pool.curve_type {
        if !depeg.depeg_type.is_none() {
            let current_time = quote_data.clock.unix_timestamp as u64;
            let cache_expire_time = depeg.base_cache_updated + BASE_CACHE_EXPIRES;
            
            if current_time > cache_expire_time {
                // 缓存已过期，但我们不在这里更新
                // 而是返回错误，提示需要先调用 update
                return Err(anyhow!(
                    "Virtual price cache expired. Please call update() first. \
                    Current time: {}, Cache expires at: {}", 
                    current_time, cache_expire_time
                ));
            }
        }
    }
    
    // 继续原有的 compute_quote 逻辑，但不调用 update_base_virtual_price
    // ...
}
```

### 3. 另一种方案：在 update 中预更新

更简单的方案是在 `update` 中直接更新 pool 并存储：

```rust
impl MeteoraAmm {
    fn update(&mut self, account_map: &AccountMap) -> Result<()> {
        // 读取 Pool
        let pool_data = try_get_account_data(account_map, &self.config.pool_address)?;
        let mut pool = deserialize_anchor_account::<Pool>(&pool_data)?;
        
        // 读取其他账户...
        
        // 构建 Clock
        let clock = Clock {
            slot: self.clock_ref.slot.load(Ordering::Relaxed),
            unix_timestamp: self.clock_ref.unix_timestamp.load(Ordering::Relaxed),
            epoch_start_timestamp: 0,
            epoch: 0,
            leader_schedule_epoch: 0,
        };
        
        // 在这里更新虚拟价格
        if let CurveType::Stable { ref mut depeg, .. } = &mut pool.curve_type {
            if !depeg.depeg_type.is_none() {
                let current_time = clock.unix_timestamp as u64;
                let cache_expire_time = depeg.base_cache_updated
                    .saturating_add(600); // BASE_CACHE_EXPIRES = 600
                
                if current_time > cache_expire_time {
                    // 获取新的虚拟价格
                    let virtual_price = match depeg.depeg_type {
                        DepegType::Marinade => {
                            // 从 stake_data 获取并计算
                            if let Some(data) = stake_data.get(&MARINADE_STATE) {
                                calculate_marinade_virtual_price(data)?
                            } else {
                                depeg.base_virtual_price // 保持旧值
                            }
                        },
                        DepegType::Lido => {
                            if let Some(data) = stake_data.get(&LIDO_STATE) {
                                calculate_lido_virtual_price(data)?
                            } else {
                                depeg.base_virtual_price
                            }
                        },
                        DepegType::SplStake => {
                            if let Some(data) = stake_data.get(&pool.stake) {
                                calculate_spl_stake_virtual_price(data)?
                            } else {
                                depeg.base_virtual_price
                            }
                        },
                        _ => depeg.base_virtual_price,
                    };
                    
                    // 更新 pool 中的值
                    depeg.base_virtual_price = virtual_price;
                    depeg.base_cache_updated = current_time;
                    
                    info!("[METEORA] Updated virtual price: {} at time {}", 
                        virtual_price, current_time);
                }
            }
        }
        
        // 存储更新后的 pool
        let cached_data = CachedQuoteData {
            pool,  // 这个 pool 已经包含最新的虚拟价格
            // ... 其他字段
        };
        
        *self.cached_data.write() = Some(cached_data);
        Ok(())
    }
}
```

## 优势

1. **性能提升**：`quote` 变成纯只读操作，更快
2. **并发友好**：多个 `quote` 可以并发执行，不需要修改数据
3. **清晰分离**：`update` 负责所有状态更新，`quote` 只负责计算
4. **缓存一致性**：虚拟价格和其他数据同时更新，保持一致

## 实现建议

### 方案 A：修改 SDK（推荐）

如果可以修改 SDK，建议：

1. 添加 `compute_quote_with_updated_pool` 函数，接受已更新的 pool
2. 或者让 `compute_quote` 检查缓存但不更新

### 方案 B：在应用层处理（当前可行）

不修改 SDK，在 meteora.rs 中：

```rust
fn update(&mut self, account_map: &AccountMap) -> Result<()> {
    // ... 读取账户 ...
    
    // 手动执行 update_base_virtual_price 的逻辑
    let mut pool = deserialize_anchor_account::<Pool>(&pool_data)?;
    
    // 构建用于更新的 QuoteData
    let temp_quote_data = QuoteData {
        pool: pool.clone(),
        vault_a: vault_a.clone(),
        vault_b: vault_b.clone(),
        pool_vault_a_lp_token: pool_vault_a_lp.clone(),
        pool_vault_b_lp_token: pool_vault_b_lp.clone(),
        vault_a_lp_mint: vault_a_lp_mint.clone(),
        vault_b_lp_mint: vault_b_lp_mint.clone(),
        vault_a_token: vault_a_token.clone(),
        vault_b_token: vault_b_token.clone(),
        clock: clock.clone(),
        stake_data: stake_data.clone(),
    };
    
    // 调用一次 compute_quote 来触发更新
    // 这会更新 pool 中的虚拟价格
    let _ = compute_quote(
        self.config.token_a_mint,
        1,  // 最小金额
        temp_quote_data,
    );
    
    // 现在 pool 已经包含最新的虚拟价格
    // 存储它
    let cached_data = CachedQuoteData {
        pool,  // 包含更新后的虚拟价格
        // ...
    };
}
```

## 监控建议

添加监控指标：

```rust
struct Metrics {
    virtual_price_updates: Counter,
    cache_hits: Counter,
    cache_misses: Counter,
    update_latency: Histogram,
    quote_latency: Histogram,
}
```

这样可以监控：
- 虚拟价格更新频率
- 缓存命中率
- 性能改善情况