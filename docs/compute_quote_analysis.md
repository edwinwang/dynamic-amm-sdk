# Dynamic AMM compute_quote 函数分析文档

## 1. 概述

`compute_quote` 函数是 Dynamic AMM SDK 中的核心函数，用于计算交易报价（swap quote）。该函数位于 `/dynamic-amm-quote/src/lib.rs:58-244`，负责计算在 AMM 池中进行代币交换时的输出金额和费用。

## 2. 函数签名

```rust
pub fn compute_quote(
    in_token_mint: Pubkey,     // 输入代币的 mint 地址
    in_amount: u64,             // 输入代币数量
    quote_data: QuoteData,      // 报价所需的所有数据
) -> anyhow::Result<QuoteResult>
```

## 3. 核心数据结构

### 3.1 QuoteData (输入数据结构)

```rust
pub struct QuoteData {
    pub pool: Pool,                              // AMM 池状态
    pub vault_a: Vault,                          // Vault A 状态
    pub vault_b: Vault,                          // Vault B 状态
    pub pool_vault_a_lp_token: TokenAccount,    // 池持有的 Vault A LP 代币账户
    pub pool_vault_b_lp_token: TokenAccount,    // 池持有的 Vault B LP 代币账户
    pub vault_a_lp_mint: Mint,                  // Vault A 的 LP mint
    pub vault_b_lp_mint: Mint,                  // Vault B 的 LP mint
    pub vault_a_token: TokenAccount,            // Vault A 的代币账户
    pub vault_b_token: TokenAccount,            // Vault B 的代币账户
    pub clock: Clock,                           // 时钟账户（系统时间）
    pub stake_data: HashMap<Pubkey, Vec<u8>>,   // 质押数据（仅用于 depeg 池）
}
```

### 3.2 QuoteResult (输出数据结构)

```rust
pub struct QuoteResult {
    pub out_amount: u64,  // 交换后得到的代币数量
    pub fee: u64,         // 交易费用（以输入代币计）
}
```

### 3.3 Pool (AMM 池状态)

位于 `/programs/dynamic-amm/src/state.rs:58-104`

```rust
pub struct Pool {
    // 基础信息
    pub lp_mint: Pubkey,         // 池的 LP 代币 mint
    pub token_a_mint: Pubkey,    // 代币 A 的 mint
    pub token_b_mint: Pubkey,    // 代币 B 的 mint
    
    // Vault 相关
    pub a_vault: Pubkey,         // Vault A 地址
    pub b_vault: Pubkey,         // Vault B 地址
    pub a_vault_lp: Pubkey,      // 池持有的 Vault A LP 账户
    pub b_vault_lp: Pubkey,      // 池持有的 Vault B LP 账户
    
    // 状态控制
    pub enabled: bool,           // 池是否启用
    pub bootstrapping: Bootstrapping,  // 启动配置
    
    // 费用相关
    pub fees: PoolFees,          // 费用配置
    pub protocol_token_a_fee: Pubkey,  // 协议费用账户 A
    pub protocol_token_b_fee: Pubkey,  // 协议费用账户 B
    
    // 曲线类型
    pub curve_type: CurveType,   // 交易曲线类型
    
    // 其他
    pub stake: Pubkey,           // 质押池地址（用于 depeg）
    pub total_locked_lp: u64,    // 锁定的 LP 数量
}
```

### 3.4 Vault (金库状态)

位于 `/programs/dynamic-vault/src/state.rs:14-41`

```rust
pub struct Vault {
    pub enabled: u8,                 // 是否启用
    pub total_amount: u64,           // 总流动性
    pub token_vault: Pubkey,         // 代币账户
    pub fee_vault: Pubkey,           // 费用账户
    pub token_mint: Pubkey,          // 支持的代币 mint
    pub lp_mint: Pubkey,             // LP mint
    pub strategies: [Pubkey; 30],   // 策略列表
    pub locked_profit_tracker: LockedProfitTracker,  // 锁定利润跟踪器
    // ... 其他字段
}
```

关键方法：
- `get_amount_by_share()`: 根据份额获取代币数量
- `get_unlocked_amount()`: 获取未锁定的金额
- `get_unmint_amount()`: 计算需要销毁的 LP 数量

### 3.5 PoolFees (费用结构)

```rust
pub struct PoolFees {
    pub trade_fee_numerator: u64,              // 交易费分子
    pub trade_fee_denominator: u64,            // 交易费分母
    pub protocol_trade_fee_numerator: u64,     // 协议费分子
    pub protocol_trade_fee_denominator: u64,   // 协议费分母
}
```

### 3.6 CurveType (曲线类型)

```rust
pub enum CurveType {
    ConstantProduct,  // 恒定乘积曲线 (x * y = k)
    Stable {          // 稳定币曲线
        amp: u64,     // 放大系数
        token_multiplier: TokenMultiplier,  // 代币乘数（处理精度）
        depeg: Depeg,                        // Depeg 信息
        last_amp_updated_timestamp: u64,    // 最后更新时间
    },
}
```

## 4. 核心算法流程

### 4.1 整体流程图

```
输入: in_token_mint, in_amount, quote_data
    ↓
1. 验证池状态
    - 检查池是否启用
    - 检查是否到达激活时间
    ↓
2. 更新 depeg 虚拟价格（如果需要）
    ↓
3. 计算 Vault 中的实际代币数量
    - token_a_amount = vault_a.get_amount_by_share()
    - token_b_amount = vault_b.get_amount_by_share()
    ↓
4. 确定交易方向
    - AtoB 或 BtoA
    ↓
5. 计算费用
    - 交易费 = trading_fee(in_amount)
    - 协议费 = protocol_trading_fee(交易费)
    - 实际交易费 = 交易费 - 协议费
    ↓
6. 计算输入代币对应的 LP 数量
    - in_lp = in_vault.get_unmint_amount()
    ↓
7. 更新 Vault 总量并计算实际输入
    - 更新 in_vault.total_amount
    - 计算 actual_in_amount
    ↓
8. 执行曲线计算
    - 根据 curve_type 选择算法
    - 计算 destination_amount_swapped
    ↓
9. 计算输出代币数量
    - out_vault_lp = out_vault.get_unmint_amount()
    - out_amount = out_vault.get_amount_by_share()
    ↓
10. 验证输出
    - 确保 out_amount < vault 储备
    ↓
输出: QuoteResult { out_amount, fee }
```

### 4.2 详细步骤解析

#### 步骤 1-2: 初始化验证
```rust
// 1. 获取激活类型和当前时间点
let activation_type = ActivationType::try_from(pool.bootstrapping.activation_type)
let current_point = match activation_type {
    ActivationType::Slot => clock.slot,
    ActivationType::Timestamp => clock.unix_timestamp as u64,
}

// 2. 验证池状态
ensure!(pool.enabled, "Pool disabled")
ensure!(current_point >= pool.bootstrapping.activation_point, "Swap is disabled")

// 3. 更新 depeg 虚拟价格（用于 LST 代币）
update_base_virtual_price(&mut pool, &clock, stake_data)
```

#### 步骤 3-4: 计算实际流动性
```rust
// 获取 Vault A 中的实际代币数量
let token_a_amount = vault_a.get_amount_by_share(
    current_time,
    pool_vault_a_lp_token.amount,  // 池持有的 LP 数量
    vault_a_lp_mint.supply,         // LP 总供应量
)

// 获取 Vault B 中的实际代币数量
let token_b_amount = vault_b.get_amount_by_share(
    current_time,
    pool_vault_b_lp_token.amount,
    vault_b_lp_mint.supply,
)

// 确定交易方向
let trade_direction = if in_token_mint == pool.token_a_mint {
    TradeDirection::AtoB
} else {
    TradeDirection::BtoA
}
```

#### 步骤 5: 费用计算
```rust
// 计算总交易费
let trade_fee = pool.fees.trading_fee(in_amount)

// 计算协议费（从交易费中抽取）
let protocol_fee = pool.fees.protocol_trading_fee(trade_fee)

// 实际交易费 = 总费用 - 协议费
let trade_fee = trade_fee - protocol_fee

// 扣除协议费后的输入金额
let in_amount_after_protocol_fee = in_amount - protocol_fee
```

#### 步骤 6-7: Vault LP 计算
```rust
// 计算输入代币对应的 LP 数量
let in_lp = in_vault.get_unmint_amount(
    current_time,
    in_amount_after_protocol_fee,
    in_vault_lp_mint.supply,
)

// 更新 Vault 总量
in_vault.total_amount += in_amount_after_protocol_fee

// 计算实际输入金额（考虑 LP 份额变化）
let after_in_token_total_amount = in_vault.get_amount_by_share(
    current_time,
    in_lp + in_vault_lp.amount,           // 新的 LP 总量
    in_vault_lp_mint.supply + in_lp,      // 新的 LP 供应量
)

let actual_in_amount = after_in_token_total_amount - before_in_token_total_amount
let actual_in_amount_after_fee = actual_in_amount - trade_fee
```

#### 步骤 8: 曲线计算

根据 `curve_type` 选择不同的算法：

**恒定乘积 (ConstantProduct)**
```rust
// x * y = k
// 实现: dy = (y * dx) / (x + dx)
let result = constant_product::swap(
    actual_in_amount_after_fee,
    in_token_total_amount,
    out_token_total_amount,
)
```

**稳定币曲线 (StableSwap)**
```rust
// 使用 StableSwap 算法（基于 Curve 的算法）
// 1. 标准化代币精度
let upscaled_amounts = match trade_direction {
    AtoB => (
        upscale_token_a(source_amount),
        upscale_token_a(swap_source_amount),
        upscale_token_b(swap_destination_amount),
    ),
    BtoA => (
        upscale_token_b(source_amount),
        upscale_token_b(swap_source_amount),
        upscale_token_a(swap_destination_amount),
    ),
}

// 2. 执行 StableSwap 计算
let result = saber_stable_swap.swap_to2(...)

// 3. 反标准化结果
let destination_amount_swapped = downscale_token(result.amount_swapped)
```

#### 步骤 9-10: 计算输出
```rust
// 计算输出 Vault 的 LP 数量
let out_vault_lp = out_vault.get_unmint_amount(
    current_time,
    destination_amount_swapped,
    out_vault_lp_mint.supply,
)

// 计算实际输出代币数量
let out_amount = out_vault.get_amount_by_share(
    current_time,
    out_vault_lp,
    out_vault_lp_mint.supply,
)

// 验证输出不超过 Vault 储备
ensure!(out_amount < out_vault_token_account.amount, "Out amount > vault reserve")
```

## 5. 特殊机制

### 5.1 Depeg 机制

Depeg 机制用于处理流动性质押代币（LST）如 mSOL、stSOL 等：

```rust
pub struct Depeg {
    pub base_virtual_price: u64,      // 虚拟价格
    pub base_cache_updated: u64,      // 缓存更新时间
    pub depeg_type: DepegType,        // 类型
}

pub enum DepegType {
    None,      // 普通池
    Marinade,  // Marinade mSOL
    Lido,      // Lido stSOL
    SplStake,  // SPL Stake Pool
}
```

虚拟价格更新逻辑：
1. 检查缓存是否过期（默认 10 分钟）
2. 从质押池获取最新虚拟价格
3. 更新缓存

### 5.2 代币精度标准化

TokenMultiplier 用于处理不同精度的代币：

```rust
pub struct TokenMultiplier {
    pub token_a_multiplier: u64,  // 代币 A 乘数
    pub token_b_multiplier: u64,  // 代币 B 乘数
    pub precision_factor: u8,     // 最高精度
}
```

例如：
- USDC (6 decimals) 和 USDT (6 decimals)：multiplier = 1
- USDC (6 decimals) 和 DAI (18 decimals)：需要标准化到相同精度

### 5.3 Vault 机制

Vault 是一个独立的资金管理层：
1. **流动性管理**：将资金分配到不同的策略中赚取收益
2. **LP 代币**：用户存入代币获得 LP，份额代表在 Vault 中的占比
3. **锁定利润**：通过 LockedProfitTracker 逐步释放收益，防止套利

```rust
// Vault 份额计算公式
amount = (share * total_amount) / total_supply

// LP 铸造量计算
unmint_amount = (out_token * total_supply) / total_amount
```

## 6. 费用机制

### 6.1 费用类型

1. **交易费 (Trade Fee)**
   - 计算：`fee = (amount * numerator) / denominator`
   - 默认：0.25% (25/10000)
   - 归属：留在池中，增加 LP 价值

2. **协议费 (Protocol Fee)**
   - 计算：从交易费中抽取一定比例
   - 默认：交易费的 20%
   - 归属：发送到协议费用账户

3. **合作伙伴费 (Partner Fee)**
   - 可选的额外费用层
   - 用于激励集成方

### 6.2 费用流程

```
用户输入 100 USDC
    ↓
计算交易费: 100 * 0.0025 = 0.25 USDC
    ↓
计算协议费: 0.25 * 0.2 = 0.05 USDC
    ↓
实际交易费: 0.25 - 0.05 = 0.2 USDC
    ↓
用于交换的金额: 100 - 0.05 = 99.95 USDC
    ↓
扣除交易费后: 99.95 - 0.2 = 99.75 USDC
    ↓
执行交换计算
```

## 7. 安全检查

1. **池状态检查**
   - 池必须启用 (`enabled = true`)
   - 必须到达激活时间

2. **代币验证**
   - 输入代币必须是池支持的代币之一

3. **数学溢出保护**
   - 所有计算使用 `checked_*` 方法
   - 返回 Option/Result 处理错误

4. **储备检查**
   - 输出金额不能超过 Vault 储备

5. **最小费用**
   - 确保至少收取 1 个最小单位的费用

## 8. 性能优化

1. **缓存机制**
   - Depeg 虚拟价格缓存 10 分钟
   - 减少链上读取

2. **批量计算**
   - 一次性获取所有需要的数据
   - 减少账户访问

3. **精度处理**
   - 使用 u128 进行中间计算
   - 最后转换为 u64

## 9. 使用示例

```rust
// 准备报价数据
let quote_data = QuoteData {
    pool: pool_account,
    vault_a: vault_a_account,
    vault_b: vault_b_account,
    pool_vault_a_lp_token: pool_vault_a_lp,
    pool_vault_b_lp_token: pool_vault_b_lp,
    vault_a_lp_mint: vault_a_lp_mint,
    vault_b_lp_mint: vault_b_lp_mint,
    vault_a_token: vault_a_token_account,
    vault_b_token: vault_b_token_account,
    clock: clock_account,
    stake_data: HashMap::new(),
};

// 计算报价
let result = compute_quote(
    usdc_mint,           // 输入 USDC
    1_000_000,           // 1 USDC (6 decimals)
    quote_data,
)?;

println!("输出金额: {} USDT", result.out_amount);
println!("交易费用: {} USDC", result.fee);
```

## 10. 总结

`compute_quote` 函数是一个复杂但设计良好的交易报价系统，主要特点：

1. **双层架构**：Pool + Vault 分离资金管理和交易逻辑
2. **灵活的曲线**：支持恒定乘积和稳定币曲线
3. **精确的费用**：多层费用机制，支持协议和合作伙伴
4. **Depeg 支持**：原生支持 LST 代币的虚拟价格
5. **安全性**：完善的验证和溢出保护

该系统适用于：
- DEX 聚合器
- 交易机器人
- 价格预言机
- 套利系统