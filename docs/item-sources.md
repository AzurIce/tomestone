# 物品来源系统

## 概述

FF14 中物品的获取来源可以从 EXD 表中追踪。本文档整理了所有已知的来源类型、对应的表结构、以及表之间的关联关系。

## 已实现的来源类型

### 1. 制作 (Recipe)

通过生产职业制作获得。数据来自 Recipe 表。

**表**: Recipe (47 列, Language::None)

| 列 | 类型 | 含义 |
|---|---|---|
| col[1] | Int32 | CraftType (0=刻木匠..7=烹调师) |
| col[2] | UInt16 | RecipeLevelTable |
| col[4] | Int32 | ItemResult (产出物品 ID) |
| col[5] | UInt8 | AmountResult (产出数量) |
| col[6..21] | (Int32, UInt8) x8 | Ingredient[0..7] 交错: (item_id, amount) |

### 2. 金币商店 (GilShop)

用金币从 NPC 购买。

**表**: GilShop + GilShopItem (子行表)

GilShop:
- col[0]: String — 商店分类名 (如"购买武器（8级）"、"购买工具")
- row_id 范围: 262144+ (0x40000+)

GilShopItem (子行表, Language::None):
- col[0]: Int32 — Item ID (商品)
- row_id = GilShop 的 row_id, subrow_id = 商品序号
- 价格从 Item 表的 PriceMid (col[25]) 获取

**NPC 关联** (反向查找):
```
ENpcBase.ENpcData[0..31] → GilShop row_id (直接)
ENpcBase.ENpcData[0..31] → TopicSelect → GilShop (间接)
ENpcResident (同 row_id) → NPC 名称
```

### 3. 特殊兑换 (SpecialShop)

用代币/物品兑换（诗学、军票、各种代币等）。

**表**: SpecialShop (2052 列, Language::ChineseSimplified)

60 个交易槽位的扁平化结构:
```
col[0]:       String  - 商店名称
对于每个槽位 i (0..59):
  col[1+i]:     Int32   - ReceiveItem[i]     (获得的 Item ID)
  col[61+i]:    UInt32  - ReceiveCount[i]     (获得数量)
  col[241+i]:   Int32   - CostItem_0[i]      (花费 Item ID)
  col[301+i]:   UInt32  - CostCount_0[i]     (花费数量)
```

### 4. 采集 (GatheringItem)

通过采矿/园艺采集获得。

**表**: GatheringItem (10 列, Language::None)

| 列 | 类型 | 含义 |
|---|---|---|
| col[0] | Int32 | Item ID 引用 |
| col[1] | UInt16 | GatheringItemLevel |
| col[2] | Bool | IsHidden |

## 尚未实现的来源类型

### 5. 军票商店 (GCShop / GCScripShopItem)

用军票兑换。row_id 范围: 1441792+ (0x160000+)。
GCScripShopItem 是子行表，包含军票兑换的物品。

### 6. 部队商店 (FccShop)

用部队积分兑换。row_id 范围: 2752512+ (0x2A0000+)。

### 7. 包含式商店 (InclusionShop)

新式商店 UI（如 Rowena 的兑换窗口）。row_id 范围: 3801088+ (0x3A0000+)。
关联链: InclusionShop → InclusionShopCategory → InclusionShopSeries → SpecialShop。

### 8. 收藏品商店 (CollectablesShop)

收藏品纳品。row_id 范围: 3866624+ (0x3B0000+)。

### 9. 副本掉落

通过 ContentFinderCondition 及相关表追踪。结构复杂，暂未实现。

### 10. 任务奖励 (Quest)

完成任务获得的物品。Quest 表中有奖励物品字段。

### 11. 成就奖励 (Achievement)

Achievement 表中有 Item 字段，表示达成成就后获得的物品。

### 12. 其他

- 分解 (Item.Desynth) — 分解其他物品获得
- 精选 (Item.AetherialReduce) — 精选获得
- 藏宝图 (TreasureHuntRank)
- 市场板 — 运行时数据，EXD 中无法追踪

## NPC 关联系统

### ENpcBase (NPC 基础数据)

row_id 范围: 1000000+

核心字段: `ENpcData[32]` — 一个长度为 32 的多态引用数组，存储该 NPC 关联的所有功能（商店、任务等）。

目标表由 row_id 范围决定:

| row_id 范围 | 表名 | 说明 |
|---|---|---|
| 0x40000 (262144+) | GilShop | 金币商店 |
| 0x160000 (1441792+) | GCShop | 军票商店 |
| 0x1B0000 (1769472+) | SpecialShop | 特殊兑换商店 |
| 0x2A0000 (2752512+) | FccShop | 部队商店 |
| 0x320000 (3276800+) | TopicSelect | 话题选择（子菜单） |
| 0x360000 (3538944+) | PreHandler | 预处理器 |
| 0x3A0000 (3801088+) | InclusionShop | 包含式商店 |
| 0x3B0000 (3866624+) | CollectablesShop | 收藏品商店 |

### ENpcResident (NPC 名称)

与 ENpcBase 共享 row_id。col[0] = NPC 名称。

### TopicSelect (话题选择)

NPC 对话中的子菜单，可包含多个商店引用。
- col[0]: String — 话题名称
- Shop[0..9]: 关联的商店 row_id (可指向 GilShop 或 SpecialShop)

### 反向索引构建方式

要找到"哪个 NPC 卖某个商店的东西"，需要反向查找:

1. 遍历所有 ENpcBase 行
2. 检查 ENpcData[0..31] 中的每个值
3. 如果值在 GilShop 范围内 → 直接关联
4. 如果值在 TopicSelect 范围内 → 查 TopicSelect.Shop[] 间接关联
5. 通过 ENpcResident (同 row_id) 获取 NPC 名称

## 当前实现的数据结构

```rust
enum ItemSource {
    /// 金币商店可购买 (价格从 Item.price_mid 获取)
    GilShop { shop_name: String },
    /// 特殊兑换 (诗学/军票/代币等)
    SpecialShop {
        shop_name: String,
        cost_item_id: u32,
        cost_count: u32,
    },
    /// 采集 (采矿/园艺)
    Gathering,
}
```

反向索引: `HashMap<u32, Vec<ItemSource>>` — item_id → 来源列表

## 辅助表

### Item 表 (91 列, Language::ChineseSimplified)

| 列 | 类型 | 含义 |
|---|---|---|
| col[0] | String | Name (名称) |
| col[8] | String | Description (描述) |
| col[10] | UInt16 | Icon ID |
| col[13] | UInt8 | FilterGroup (物品大类) |
| col[14] | UInt32 | AdditionalData |
| col[15] | UInt8 | ItemUICategory |
| col[17] | UInt8 | EquipSlotCategory |
| col[25] | UInt32 | PriceMid (NPC 收购价) |
| col[26] | UInt32 | PriceLow (NPC 卖出价) |
| col[47] | UInt64 | ModelMain |

### ItemUICategory 表 (4 列, Language::ChineseSimplified)

| 列 | 类型 | 含义 |
|---|---|---|
| col[0] | String | Name (分类名, 如 "炼金术材料") |
