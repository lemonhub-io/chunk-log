# 修复完成审计

审计日期：2026-08-09  
审计对象：`chunklog` 0.3.0 实现、论文、实验与复现材料
结论：修复计划中影响论文真实性和数据安全的 P0/P1 问题已经关闭；论文主张已缩小到现有证明与归档实验能够支持的范围。

## 已关闭事项

- 平面 Tree 被固定深度 16-nibble 持久化 radix Merkle Tree 替代；完整快照与显式 change set 使用不同 API 和复杂度陈述。
- Blob、Branch、Leaf、Commit 采用带 magic、版本及类型 tag 的规范编码；所有读取验证 BLAKE3 地址。
- 所有分支入口拒绝路径穿越；写操作由仓库锁串行化，并在持锁后刷新 HEAD。
- HEAD 和分支引用使用同目录临时文件与原子替换；论文不再把多文件 commit 称为事务。
- CLI staging 变成显式 upsert/removal patch，不再把局部 staging 当作完整世界。
- GC 在 sweep 前完成所有可达对象的验证和标记；marking 失败不删除，sweep 失败可安全重试但不宣称 crash atomicity。
- 未版本化旧实验仓库及未知格式被明确拒绝，避免静默误读。
- benchmark 生成器断言 N 个 payload 全部唯一；算法、文件系统、load、checkout 与 raw-file baseline 分组测量。
- 论文的算法模型、定理、摘要、相关工作、实验、限制和结论均已按实现重写。
- 已归档结构增长实验、Criterion 中位数/区间，以及官方 Luanti 5.16.1 生成的真实二进制 mapblock 导入实验。

## 最终工程验收

在 Windows NTFS、Rust 1.97.1、独立 `CARGO_TARGET_DIR` 中执行：

```text
cargo fmt --all -- --check                         PASS
cargo clippy --all-targets --offline -- -D warnings PASS
cargo test --offline --no-fail-fast                PASS
cargo test --all-targets --offline --no-run        PASS
cargo doc --no-deps --offline                      PASS
rustup 1.86: cargo check/test --locked --offline   PASS
git diff --check                                   PASS
```

测试结果为 2 个 CLI 单元测试、45 个集成测试和 1 个 doctest，全部通过。集成测试包含 100 轮确定性状态机、合法对象全截断语料、伪随机解码语料、内容篡改、路径穿越、writer lock、增量路径上界、SQLite commit/rollback、对象事务失败时禁止发布 HEAD，以及两类 GC 故障注入。CI 保留 MSRV 1.86 与覆盖率任务，并将主测试扩展为 Ubuntu/Windows 矩阵；该矩阵需在提交到 GitHub 后由远端实际执行。

## 实验归档

- `paper-results/benchmark-summary.md`：Criterion 0.8.2 配置、机器信息、中位数和置信区间。
- `paper-results/criterion-medians.csv`：可机器读取的汇总数据。
- `paper-results/structural-growth.md`：N=1,024、R=50、k∈{1,10,100} 的分类型对象及字节增长。
- `paper-results/luanti-workload.md`：2,023 个 Luanti mapblock 聚合为 289 个列 payload 的结果。
- `paper-workloads/run-luanti.ps1`：真实引擎生成和导入的复现入口。

## 0.3.0 初始导入专项修复

- 根因：N=1,000 的持久化树会生成大量小型 Branch/Leaf/Blob；loose store 对每个对象执行独立 create/write/rename，NTFS 元数据操作支配总耗时。
- 修复：默认 `SqliteStore` 将完整 canonical object 存入单个 `WITHOUT ROWID` 表；一次 repository commit 对应一个 `BEGIN IMMEDIATE`/`COMMIT` 对象事务，之后才发布 ref。
- 同版本 Criterion 对照：N=1,000 loose-file 中位数 10,427.392 ms，SQLite 中位数 47.433 ms，约 220× 加速。
- 扩展规模：SQLite N=10,000 初始导入中位数 1,156.306 ms。
- 回归检查：k=1 提交为 10.446/13.577 ms，load 为 21.221/222.167 ms，logical checkout 为 5.225/7.765 ms（N=100/1,000）。

## 明确不作为 0.3.0 主张的事项

下列事项不是隐藏的未关闭缺陷，而是论文已经公开陈述的边界：

- SQLite 使用 rollback journal 与 `synchronous=FULL`，但对象事务与文件系统 ref 不构成一个跨文件事务。
- 默认 SQLite GC sweep 是事务性的；generic/loose store 只保证可达对象安全及重试性。
- 不提供无版本旧实验格式的迁移命令；此格式缺少可靠的类型/版本判据，当前策略是 fail closed。
- Luanti 实验是受控 singlenode 序列化兼容性实验，不代表生产玩家编辑历史或生产性能。
- SQLite 已消除主要 small-file 瓶颈，但单主机 synthetic 结果仍不足以主张生产竞争力。
- logical checkout 不包含世界物化和引擎激活。

这些边界也同步记录在 `README.md`、`FORMAT.md`、`paper.md` 与 `CLAIMS_EVIDENCE.md` 中。
