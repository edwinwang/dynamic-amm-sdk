# Meteora Dynamic AMM 指令账户详细分析

## 1. 核心交易指令

### 1.1 Swap 指令

**路径**: `programs/dynamic-amm/src/instructions/swap.rs`

#### 账户列表及权限
```rust
#[derive(Accounts)]
pub struct Swap<'info> {
    #[account(mut)] pool: UncheckedAccount<'info>,                  // Pool 主账户
    #[account(mut)] user_source_token: UncheckedAccount<'info>,     // 用户源代币账户
    #[account(mut)] user_destination_token: UncheckedAccount<'info>,// 用户目标代币账户
    #[account(mut)] a_vault: UncheckedAccount<'info>,               // Vault A
    #[account(mut)] b_vault: UncheckedAccount<'info>,               // Vault B
    #[account(mut)] a_token_vault: UncheckedAccount<'info>,         // Vault A 代币账户
    #[account(mut)] b_token_vault: UncheckedAccount<'info>,         // Vault B 代币账户
    #[account(mut)] a_vault_lp_mint: UncheckedAccount<'info>,       // Vault A LP mint
    #[account(mut)] b_vault_lp_mint: UncheckedAccount<'info>,       // Vault B LP mint
    #[account(mut)] a_vault_lp: UncheckedAccount<'info>,            // Pool 持有的 Vault A LP
    #[account(mut)] b_vault_lp: UncheckedAccount<'info>,            // Pool 持有的 Vault B LP
    #[account(mut)] protocol_token_fee: UncheckedAccount<'info>,    // 协议费账户
    user: Signer<'info>,                                            // 用户签名
    vault_program: UncheckedAccount<'info>,                         // Vault 程序
    token_program: UncheckedAccount<'info>,                         // Token 程序
}
```

#### 数据变动分析

**基于 compute_quote 逻辑推导的变动**：

1. **Pool 账户变动**:
   - `fees.fee_last_updated_at`: 更新为当前时间戳
   - `partner_info.pending_fee_a/b`: 累加合作伙伴费用（如果有）
   - `curve_type.depeg.base_virtual_price`: 更新虚拟价格（depeg池，如果缓存过期）
   - `curve_type.depeg.base_cache_updated`: 更新缓存时间（depeg池）

2. **Vault A/B 账户变动**:
   ```rust
   // 输入 Vault (假设 A → B)
   vault_a.total_amount += in_amount_after_protocol_fee
   
   // 输出 Vault
   vault_b.total_amount -= out_amount
   ```

3. **Vault LP Mint 变动**:
   ```rust
   // 输入侧
   a_vault_lp_mint.supply += in_lp  // 铸造新 LP
   
   // 输出侧
   b_vault_lp_mint.supply -= out_lp  // 销毁 LP
   ```

4. **Pool Vault LP 账户变动**:
   ```rust
   // Pool 持有的 LP 数量
   a_vault_lp.amount += in_lp   // 收到新铸造的 LP
   b_vault_lp.amount -= out_lp  // 销毁用于提取的 LP
   ```

5. **Token Vault 账户变动**:
   ```rust
   a_token_vault.amount += in_amount_after_protocol_fee
   b_token_vault.amount -= out_amount
   ```

6. **协议费账户变动**:
   ```rust
   protocol_token_fee.amount += protocol_fee
   ```

#### 对 Quote 计算的影响

**关键影响**:
1. **立即影响**: 
   - Token amounts: `vault.get_amount_by_share()` 结果变化
   - 储备比例: 影响下一次交换的价格

2. **计算公式影响**:
   ```rust
   // 交换前
   token_a_amount = vault_a.get_amount_by_share(
       pool_vault_a_lp_token.amount,  // 变化
       vault_a_lp_mint.supply          // 变化
   )
   
   // 交换后这些值都会改变，影响下次 quote
   ```

3. **滑点累积**: 连续交换会累积滑点

---

## 2. 流动性管理指令

### 2.1 Add Balance Liquidity（平衡添加流动性）

**路径**: `programs/dynamic-amm/src/instructions/add_balance_liquidity.rs`

#### 账户权限
```rust
#[derive(Accounts)]
pub struct AddOrRemoveBalanceLiquidity<'info> {
    #[account(mut)] pool: UncheckedAccount<'info>,
    #[account(mut)] lp_mint: UncheckedAccount<'info>,          // Pool LP mint
    #[account(mut)] user_pool_lp: UncheckedAccount<'info>,     // 用户 Pool LP
    #[account(mut)] a_vault_lp: UncheckedAccount<'info>,
    #[account(mut)] b_vault_lp: UncheckedAccount<'info>,
    #[account(mut)] a_vault: UncheckedAccount<'info>,
    #[account(mut)] b_vault: UncheckedAccount<'info>,
    #[account(mut)] a_vault_lp_mint: UncheckedAccount<'info>,
    #[account(mut)] b_vault_lp_mint: UncheckedAccount<'info>,
    #[account(mut)] a_token_vault: UncheckedAccount<'info>,
    #[account(mut)] b_token_vault: UncheckedAccount<'info>,
    #[account(mut)] user_a_token: UncheckedAccount<'info>,
    #[account(mut)] user_b_token: UncheckedAccount<'info>,
    user: Signer<'info>,
    vault_program: UncheckedAccount<'info>,
    token_program: UncheckedAccount<'info>,
}
```

#### 数据变动

**添加流动性时**:
1. **Pool LP Mint**:
   ```rust
   lp_mint.supply += mint_amount  // 铸造 Pool LP 给用户
   ```

2. **Vault 变动**:
   ```rust
   vault_a.total_amount += token_a_amount
   vault_b.total_amount += token_b_amount
   ```

3. **Vault LP**:
   ```rust
   // 按比例铸造 Vault LP
   a_vault_lp_mint.supply += a_lp_amount
   b_vault_lp_mint.supply += b_lp_amount
   
   // Pool 收到 Vault LP
   a_vault_lp.amount += a_lp_amount
   b_vault_lp.amount += b_lp_amount
   ```

4. **Token Vault**:
   ```rust
   a_token_vault.amount += token_a_amount
   b_token_vault.amount += token_b_amount
   ```

#### 对 Quote 的影响
- **储备增加**: 两边储备等比例增加，价格不变
- **深度改善**: 相同滑点下可交换更多
- **LP 供应增加**: 影响份额计算

### 2.2 Remove Balance Liquidity（平衡移除流动性）

#### 数据变动（与添加相反）
```rust
// 销毁 Pool LP
lp_mint.supply -= burn_amount

// 减少 Vault 总量
vault_a.total_amount -= token_a_amount
vault_b.total_amount -= token_b_amount

// 销毁 Vault LP
a_vault_lp_mint.supply -= a_lp_amount
b_vault_lp_mint.supply -= b_lp_amount
a_vault_lp.amount -= a_lp_amount
b_vault_lp.amount -= b_lp_amount

// 减少代币
a_token_vault.amount -= token_a_amount
b_token_vault.amount -= token_b_amount
```

### 2.3 Add Imbalance Liquidity（不平衡添加）

**特点**: 只有稳定币池支持

#### 额外变动
- **价格影响**: 不平衡添加会改变储备比例
- **费用**: 可能收取不平衡费用
- **滑点**: 造成立即的价格偏移

### 2.4 Remove Liquidity Single Side（单边移除）

**特点**: 只有稳定币池支持

#### 数据变动
```rust
// 只影响一边的储备
if (remove_token_a) {
    vault_a.total_amount -= amount
    a_token_vault.amount -= amount
    // 销毁对应的 Vault LP
} else {
    vault_b.total_amount -= amount
    b_token_vault.amount -= amount
    // 销毁对应的 Vault LP
}
```

#### 对 Quote 的影响
- **严重价格影响**: 单边移除造成储备不平衡
- **套利机会**: 可能创造套利空间

---

## 3. 池管理指令

### 3.1 Initialize Pool（初始化池）

**账户创建**:
- 创建 Pool 账户
- 创建 LP Mint
- 创建协议费账户
- 关联 Vault

**初始数据**:
```rust
pool = Pool {
    enabled: true,
    token_a_mint,
    token_b_mint,
    a_vault,
    b_vault,
    fees: initial_fees,
    curve_type,
    bootstrapping: {
        activation_point,
        activation_type,
    },
    // ...
}
```

### 3.2 Enable/Disable Pool

#### 数据变动
```rust
pool.enabled = enable  // true 或 false
```

#### 对 Quote 的影响
- **pool.enabled = false**: Quote 直接失败
- **关键检查**: `ensure!(pool.enabled, "Pool disabled")`

### 3.3 Set Pool Fees

#### 数据变动
```rust
pool.fees = PoolFees {
    trade_fee_numerator: new_numerator,
    trade_fee_denominator: new_denominator,
    protocol_trade_fee_numerator: new_protocol_numerator,
    protocol_trade_fee_denominator: new_protocol_denominator,
}
pool.fee_last_updated_at = current_time
```

#### 对 Quote 的影响
- **直接影响费用计算**:
  ```rust
  trade_fee = in_amount * numerator / denominator
  ```
- **影响净输出**: 费用越高，输出越少

### 3.4 Override Curve Param（仅稳定币池）

#### 数据变动
```rust
if let CurveType::Stable { amp, .. } = &mut pool.curve_type {
    *amp = new_amp
    last_amp_updated_timestamp = current_time
}
```

#### 对 Quote 的影响
- **Amp 系数影响**:
  - 高 Amp: 更像恒定价格（1:1）
  - 低 Amp: 更像恒定乘积
- **立即价格变化**: Amp 改变立即影响交换率

### 3.5 Update Activation Point

#### 数据变动
```rust
pool.bootstrapping.activation_point = new_activation_point
```

#### 对 Quote 的影响
- **激活前**: 所有 swap 被拒绝
- **激活后**: 正常交易

---

## 4. 锁定和费用指令

### 4.1 Lock LP

#### 账户变动
```rust
// 创建或更新 LockEscrow 账户
lock_escrow.total_locked_amount += amount
lock_escrow.lp_per_token = new_virtual_price

// Pool 更新
pool.total_locked_lp += amount
```

#### 对 Quote 的影响
- **无直接影响**: 锁定不影响储备
- **间接影响**: 减少流通 LP，可能影响治理

### 4.2 Claim Fee

#### 账户变动
```rust
// 转移累积的费用
protocol_token_a_fee.amount -= claim_amount_a
protocol_token_b_fee.amount -= claim_amount_b

// 更新锁定托管
lock_escrow.unclaimed_fee_pending = 0
lock_escrow.a_fee += claim_amount_a
lock_escrow.b_fee += claim_amount_b
```

#### 对 Quote 的影响
- **无影响**: 只是费用分配，不影响池储备

### 4.3 Partner Claim Fees

#### 账户变动
```rust
pool.partner_info.pending_fee_a = 0
pool.partner_info.pending_fee_b = 0
// 转移费用到合作伙伴账户
```

---

## 5. 特殊操作指令

### 5.1 Bootstrap Liquidity

**用途**: 池耗尽后重新注入流动性

#### 数据变动
- 类似初始化，但保留池配置
- 重置储备到新值

### 5.2 Create Mint Metadata

**用途**: 为旧池创建元数据

#### 对 Quote 的影响
- **无影响**: 纯元数据操作

---

## 6. 指令优先级分类

### 高优先级（直接影响价格）
1. **swap**: 改变储备比例 ⚡
2. **add_imbalance_liquidity**: 改变比例 ⚡
3. **remove_liquidity_single_side**: 改变比例 ⚡

### 中优先级（影响深度）
1. **add_balance_liquidity**: 增加深度 🔄
2. **remove_balance_liquidity**: 减少深度 🔄

### 低优先级（配置变更）
1. **set_pool_fees**: 费用调整 ⚙️
2. **override_curve_param**: Amp 调整 ⚙️
3. **enable_or_disable_pool**: 开关池 ⚙️

### 无影响
1. **lock/unlock**: 锁定操作 🔒
2. **claim_fee**: 费用领取 💰
3. **create_metadata**: 元数据 📝

---

## 7. 关键监控建议

### 实时监控账户组
```rust
// 这些账户的变化直接影响报价
critical_accounts = [
    pool.a_vault_lp,        // Pool 持有的 LP A
    pool.b_vault_lp,        // Pool 持有的 LP B
    vault_a.lp_mint,        // Vault A LP 供应
    vault_b.lp_mint,        // Vault B LP 供应
    vault_a.token_vault,    // Vault A 代币
    vault_b.token_vault,    // Vault B 代币
    vault_a,                // Vault A 状态
    vault_b,                // Vault B 状态
]
```

### 监控策略
1. **WebSocket 订阅**: 订阅 critical_accounts
2. **交易监听**: 监听包含这些账户的交易
3. **差异计算**: 实时计算储备变化
4. **机会识别**: 储备比例偏离时触发套利

---

## 8. Quote 计算依赖总结

### 核心依赖
```rust
quote_dependencies = {
    // 必需账户
    pool: ["enabled", "fees", "curve_type", "bootstrapping"],
    vault_a: ["total_amount", "locked_profit_tracker"],
    vault_b: ["total_amount", "locked_profit_tracker"],
    pool_vault_a_lp: ["amount"],
    pool_vault_b_lp: ["amount"],
    vault_a_lp_mint: ["supply"],
    vault_b_lp_mint: ["supply"],
    
    // 条件依赖
    depeg_state: ["virtual_price"],  // 仅 depeg 池
    clock: ["unix_timestamp", "slot"],
}
```

### 计算流程影响点
1. **储备计算**: `get_amount_by_share()` 依赖 LP 数量和供应
2. **费用计算**: 依赖 `pool.fees`
3. **曲线计算**: 依赖 `curve_type` 和储备
4. **激活检查**: 依赖 `bootstrapping`

这份分析基于代码结构和 compute_quote 逻辑推导，准确识别了每个指令的账户操作和对报价的影响。