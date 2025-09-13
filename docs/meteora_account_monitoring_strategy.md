# Meteora Dynamic AMM 账户监控策略文档

## 1. 概述

本文档针对 Meteora Dynamic AMM 的套利程序，详细分析了需要监控的账户及其更新频率。通过代码分析（非推测），识别了所有会被链上指令修改的账户，并根据实际用途制定监控策略。

## 2. 账户分类及监控策略

### 2.1 高频监控账户（实时监控）

这些账户在每次交易时都会变化，需要实时监控：

#### 2.1.1 Pool Vault LP 账户
- **账户**: `pool.a_vault_lp`, `pool.b_vault_lp`  
- **类型**: TokenAccount
- **变化原因**: 
  - `swap` 指令：铸造/销毁 LP（代码位置：swap.rs:39-43）
  - `add_balance_liquidity` 指令：铸造 LP
  - `remove_balance_liquidity` 指令：销毁 LP
  - `add_imbalance_liquidity` 指令：铸造 LP
  - `remove_liquidity_single_side` 指令：销毁 LP
- **监控频率**: 实时（每个区块）
- **用途**: 计算池中实际代币数量

#### 2.1.2 Vault LP Mint 账户
- **账户**: `vault_a.lp_mint`, `vault_b.lp_mint`
- **类型**: Mint
- **变化原因**: 
  - LP supply 在每次 deposit/withdraw 时变化
  - 影响 `get_amount_by_share()` 计算（state.rs:45-58）
- **监控频率**: 实时（每个区块）
- **用途**: 计算 LP 总供应量，用于份额计算

#### 2.1.3 Vault Token 账户
- **账户**: `vault_a.token_vault`, `vault_b.token_vault`
- **类型**: TokenAccount
- **变化原因**:
  - `swap` 指令：代币进出（swap.rs:26-29）
  - 流动性操作：代币进出
- **监控频率**: 实时（每个区块）
- **用途**: Vault 储备金额

#### 2.1.4 Vault State 账户
- **账户**: `pool.a_vault`, `pool.b_vault`
- **类型**: Vault（自定义账户）
- **变化原因**:
  - `total_amount` 字段在每次操作时更新（state.rs:22）
  - `locked_profit_tracker` 随时间变化（state.rs:40）
- **监控频率**: 实时（每个区块）
- **用途**: 计算未锁定金额

### 2.2 中频监控账户（分钟级）

#### 2.2.1 Pool State 账户
- **账户**: Pool 主账户
- **类型**: Pool（自定义账户）
- **主要变化字段**:
  - `enabled`: 池启用状态（enable_pool.rs）
  - `fees`: 费用设置（set_pool_fee.rs）
  - `curve_type.amp`: 稳定币池的放大系数（override_curve_param.rs）
  - `bootstrapping.activation_point`: 激活时间（update_activation_point.rs）
- **监控频率**: 1-5 分钟
- **用途**: 池配置参数

#### 2.2.2 Depeg 相关账户（如适用）
- **Marinade State**: `8szGkuLTAux9XMgZ2vtY39jVSowEcpBfFfD8hXSEqdGC`
- **Lido State**: `49Yi1TKkNyYjPAFdR9LBvoHcUjuPX4Df5T5yv39w2XTn`
- **SPL Stake Pool**: `pool.stake`
- **变化原因**: 
  - 虚拟价格更新（depeg/mod.rs:30-57）
  - 缓存过期时间：600秒（10分钟）
- **监控频率**: 10分钟
- **用途**: LST 代币虚拟价格

### 2.3 低频监控账户（小时级或一次性）

#### 2.3.1 静态配置账户
- **LP Mint**: `pool.lp_mint`
- **Token Mints**: `pool.token_a_mint`, `pool.token_b_mint`
- **Protocol Fee账户**: `pool.protocol_token_a_fee`, `pool.protocol_token_b_fee`
- **监控频率**: 启动时获取一次
- **用途**: 静态配置，基本不变

#### 2.3.2 Config 账户
- **类型**: Config（state.rs:316-329）
- **变化原因**: 仅管理员操作
- **监控频率**: 每小时或启动时
- **用途**: 全局配置

## 3. 指令对账户的影响分析

### 3.1 Swap 指令（最重要）
```rust
// 影响的账户（swap.rs）
#[account(mut)] pool                    // 更新费用统计
#[account(mut)] user_source_token        // 用户账户（不需监控）
#[account(mut)] user_destination_token   // 用户账户（不需监控）
#[account(mut)] a_vault                  // Vault状态更新
#[account(mut)] b_vault                  // Vault状态更新
#[account(mut)] a_token_vault            // 代币转移
#[account(mut)] b_token_vault            // 代币转移
#[account(mut)] a_vault_lp_mint          // LP供应量变化
#[account(mut)] b_vault_lp_mint          // LP供应量变化
#[account(mut)] a_vault_lp               // LP代币变化
#[account(mut)] b_vault_lp               // LP代币变化
#[account(mut)] protocol_token_fee       // 协议费用
```

### 3.2 流动性指令
- `add_balance_liquidity`: 影响 LP mint、vault LP、token vault
- `remove_balance_liquidity`: 影响 LP mint、vault LP、token vault
- `add_imbalance_liquidity`: 影响所有流动性相关账户
- `remove_liquidity_single_side`: 影响单边流动性账户

### 3.3 管理指令（低频）
- `set_pool_fees`: 更新 pool.fees
- `enable_or_disable_pool`: 更新 pool.enabled
- `override_curve_param`: 更新 curve_type 参数
- `update_activation_point`: 更新激活时间

## 4. 套利程序的正确账户获取方式

基于您的代码分析，以下是修正后的账户获取策略：

### 4.1 Quote 计算所需账户

```rust
// 必需的账户（按优先级）
1. pool 账户 - 基础配置
2. vault_a, vault_b - Vault 状态
3. pool_vault_a_lp, pool_vault_b_lp - Pool 持有的 LP
4. vault_a_lp_mint, vault_b_lp_mint - LP Mint 供应量
5. vault_a_token, vault_b_token - Vault 代币储备
6. clock - 系统时间
7. stake_data（如果是 depeg 池）- 虚拟价格
```

### 4.2 实时监控实现建议

```rust
// 高频账户组（WebSocket 订阅）
let high_freq_accounts = vec![
    pool.a_vault_lp,
    pool.b_vault_lp,
    vault_a.lp_mint,
    vault_b.lp_mint,
    vault_a.token_vault,
    vault_b.token_vault,
    pool.a_vault,
    pool.b_vault,
];

// 中频账户组（定时批量获取）
let medium_freq_accounts = vec![
    pool_address,
    // Depeg 相关账户（如适用）
];

// 低频账户组（缓存）
let static_accounts = vec![
    pool.lp_mint,
    pool.token_a_mint,
    pool.token_b_mint,
];
```

## 5. 优化建议

### 5.1 账户获取优化

1. **批量获取**: 使用 `get_multiple_accounts` 一次获取多个账户
2. **缓存策略**: 
   - 静态账户：永久缓存
   - Depeg 虚拟价格：10分钟缓存
   - Pool 配置：5分钟缓存
3. **WebSocket 订阅**: 对高频账户使用 accountSubscribe

### 5.2 计算优化

1. **预计算**: 在账户更新时立即计算 token amounts
2. **增量更新**: 只更新变化的部分
3. **并行处理**: 多个池的更新可以并行

### 5.3 错误处理

```rust
// 关键检查点
1. 池启用状态：pool.enabled
2. 激活时间：current_point >= pool.bootstrapping.activation_point  
3. Vault 储备：out_amount < vault_token_account.amount
4. LP 供应量：lp_supply > 0
```

## 6. 监控架构图

```
┌─────────────────────────────────────────────────┐
│                  实时监控层                      │
│  WebSocket订阅: Vault LP, Token Accounts, Mints │
└─────────────────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────┐
│                  缓存层                          │
│  - 静态账户（永久）                              │
│  - Pool配置（5分钟）                            │
│  - Depeg价格（10分钟）                          │
└─────────────────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────┐
│                  计算层                          │
│  - compute_quote 实时计算                       │
│  - 预计算 token amounts                         │
└─────────────────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────┐
│                  套利执行层                      │
│  - 机会识别                                     │
│  - 交易构建                                     │
│  - 执行监控                                     │
└─────────────────────────────────────────────────┘
```

## 7. 代码修复建议

根据您的 meteora.rs 代码，主要问题在 `get_accounts_to_update()` 方法：

### 7.1 当前问题
- 缺少关键账户（如 Clock）
- 账户顺序可能不正确
- 没有正确处理 Depeg 账户

### 7.2 修正版本

```rust
fn get_accounts_to_update(&self) -> Vec<Pubkey> {
    let state = self.state.read();
    
    // 关键顺序：按照 quote 函数的需求
    let mut accounts = vec![
        // 1. Pool 主账户（获取最新配置）
        self.key,
        
        // 2. Vault LP 账户（Pool 持有的）
        state.pool.a_vault_lp,
        state.pool.b_vault_lp,
        
        // 3. Vault 账户本身
        state.pool.a_vault,
        state.pool.b_vault,
    ];
    
    // 4. 如果已知 Vault 信息，添加相关账户
    if let Some(vault_a) = &state.vault_a_info {
        accounts.push(vault_a.vault.lp_mint);      // LP Mint
        accounts.push(vault_a.vault.token_vault);  // Token Vault
    }
    
    if let Some(vault_b) = &state.vault_b_info {
        accounts.push(vault_b.vault.lp_mint);      // LP Mint
        accounts.push(vault_b.vault.token_vault);  // Token Vault
    }
    
    // 5. Depeg 相关账户
    if let CurveType::Stable { depeg, .. } = &state.pool.curve_type {
        if !depeg.is_none() {
            match depeg.depeg_type {
                DepegType::Marinade => {
                    accounts.push(pubkey!("8szGkuLTAux9XMgZ2vtY39jVSowEcpBfFfD8hXSEqdGC"));
                },
                DepegType::Lido => {
                    accounts.push(pubkey!("49Yi1TKkNyYjPAFdR9LBvoHcUjuPX4Df5T5yv39w2XTn"));
                },
                DepegType::SplStake => {
                    accounts.push(state.pool.stake);
                },
                _ => {}
            }
        }
    }
    
    // 去重
    accounts.dedup();
    accounts
}
```

## 8. 总结

1. **高频监控**（实时）：Vault LP、Token账户、LP Mint - 这些直接影响价格计算
2. **中频监控**（分钟级）：Pool配置、Depeg价格 - 影响计算参数
3. **低频监控**（小时级）：静态配置 - 基本不变

关键是正确识别和监控会影响 `compute_quote` 计算的账户，特别是：
- Pool Vault LP 数量（pool_vault_a_lp_token.amount）
- Vault LP 总供应量（vault_a_lp_mint.supply）
- Vault 总金额（vault.total_amount）
- Vault 锁定利润（locked_profit_tracker）

这些账户的实时变化直接影响套利机会的识别。