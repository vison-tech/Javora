# 上下文自动压缩功能

## 概述

当会话上下文超过 200k tokens 时，Javora 自动调用 AI 模型生成会话摘要，保留最重要的信息，同时保留最近 5 轮对话。

## 工作原理

### 1. Token 估算
- 使用简单启发式：字符数 ÷ 4 ≈ token 数
- 统计范围：所有对话历史 + 用户上下文 + 最后命令结果
- 阈值：MAX_CONTEXT_TOKENS = 200,000

### 2. 触发时机
自动压缩在以下两个关键时刻触发：
- 用户提交新需求时（`begin_or_continue_requirements`）
- 执行已批准的实现方案时（`execute_approved`）

### 3. 压缩过程

```rust
// 1. 检查是否需要压缩
if session.estimate_tokens() > MAX_CONTEXT_TOKENS {
    // 2. 调用模型生成摘要
    let summary = generate_summary(conversation_history);
    
    // 3. 保留最近 5 轮对话
    let recent_turns = conversation.last(5);
    
    // 4. 替换历史
    conversation = [compressed_summary_turn] + recent_turns;
    
    // 5. 增加压缩计数
    compression_count += 1;
}
```

### 4. 摘要生成
使用独立的模型请求生成摘要：
- 系统提示词：专门用于压缩会话的简洁指令
- 输入：完整对话历史（带类型标签和预览）
- 输出要求：
  1. 用户的主要目标和需求
  2. 已完成的关键操作
  3. 重要的决策和结论
  4. 当前进度和待办事项
- 限制：max_tokens=1000（避免摘要本身过长）

### 5. 压缩标记
压缩后的摘要以特殊格式保存：
```
[自动压缩 #1 - 1726569600]
用户目标：实现订单创建功能
已完成：需求澄清、方案设计
待办：实现代码、编写测试
```

## 数据结构

### Session 新增字段
```rust
struct Session {
    // ... 现有字段
    compression_count: u32,  // 累计压缩次数
}
```

### 状态文件格式 (VERSION: 3)
```
VERSION: 3
STATE: CLARIFYING
PLAN_VERSION: 1
COMPRESSION_COUNT: 2
TRANSCRIPT: <hex>
USER_CONTEXT: <hex>
CONVERSATION:
AR|1726569500|system|<compressed-summary-hex>
UR|1726569600|user|<recent-turn-1-hex>
AQ|1726569610|assistant|<recent-turn-2-hex>
...
```

## 配置常量

```rust
const MAX_CONTEXT_TOKENS: usize = 200_000;        // 触发阈值
const COMPRESSED_CONTEXT_TARGET: usize = 100_000; // 目标大小（预留）
```

## 实现位置

- Token 估算: `Session::estimate_tokens()` (~L203)
- 压缩检查: `Session::should_compress()` (~L214)
- 压缩逻辑: `Session::compress_context()` (~L218)
- 摘要生成: `Session::generate_summary()` (~L248)
- 模型请求: `compress_request()` (~L881)
- 触发点 1: `begin_or_continue_requirements()` (~L527)
- 触发点 2: `execute_approved()` (~L593)

## 优势

1. **无感知**：自动触发，用户无需手动干预
2. **智能保留**：保留最近对话，避免丢失当前上下文
3. **持久化**：压缩后的状态可跨会话恢复
4. **可追溯**：压缩次数和时间戳记录在摘要中
5. **渐进式**：每次只处理历史部分，不影响当前交互

## 未来优化方向

1. **自适应保留数量**：根据对话重要性动态调整保留轮数
2. **多级压缩**：对多次压缩的历史进行二次压缩
3. **重要性评分**：为对话标记重要性，优先保留关键信息
4. **分段压缩**：将长对话按主题分段，分别压缩
5. **本地摘要**：使用更快的本地模型进行压缩，减少 API 调用
