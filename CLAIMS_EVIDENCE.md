# 论文主张—证据矩阵

更新日期：2026-08-09  
用途：约束 `paper.md`、实现、测试和归档实验之间的一致性。实验结果只说明观测到的实现行为，不替代算法证明。

| 主张 | 精确定义或前提 | 实现证据 | 测试或实验 | 状态 | 论文处理 |
| --- | --- | --- | --- | --- | --- |
| 内容地址完整性 | 读取后重新计算完整 canonical object 的 BLAKE3；不同对象类型使用不同 tag | `src/object.rs`、`src/store.rs` | 篡改对象测试、round-trip、类型域分离测试 | 已支持 | 保留；明确可信哈希及存储错误前提 |
| 规范编码确定性 | 固定 magic/version/tag、big-endian、排序分支、长度前缀、拒绝尾随数据 | `Object::to_bytes/from_bytes`、`FORMAT.md` | object/hash 集成测试 | 已支持 | 不再依赖 Serde/postcard 的隐式格式 |
| 完整快照提交 Θ(N) | 对 N 个输入 payload 编码、哈希并构建 radix Tree | `commit_snapshot` | `algorithm/full_snapshot_memory`，N=100/1,000/10,000 | 已支持并已测量 | 明确否定旧的端到端 O(k) 主张 |
| 增量提交更新路径并集 | 固定 16 层 radix Tree；k 个坐标至多新增 16k Branch、k Leaf、k Blob 和一个 Commit（内容复用前） | `commit_changes`、`apply_tree_changes` | 单路径对象上界、100 轮状态机、k=1 benchmark、50-save artifact | 已支持并已测量 | 给出精确节点上界，不把完整快照 API 混为增量 API |
| 未提及坐标保持不变 | change set 仅含显式 upsert/removal | `ChangeSet`、`commit_changes` | 保持性测试、100 轮确定性状态机 | 已支持 | 保留 |
| CLI staging 是增量 patch | `<x>,<z>` 为 upsert，`.remove` 为显式删除 | `src/cli/commit.rs` | 两个 CLI 单元测试 | 已支持 | 已删除“完整 staging 即完整世界”的旧语义 |
| logical checkout 相对 N 为 O(1) | 验证一个有界 Commit 并原子重写 HEAD；不遍历 Tree/Blob | `checkout` | N=100/1,000 logical-checkout benchmark | 已支持并已测量 | 明确排除 materialization，并限定 Commit 元数据大小 |
| 任意完整版本可比较 | 两个版本均可遍历为 coordinate→blob map | `diff`、`tree_entries` | diff 集成测试 | 已支持 | 不宣称基础理论创新；尚未实现 Merkle recursive diff |
| GC soundness | writer lock 下固定根集合；验证并标记全部 Commit/Tree/Blob 后只删未标记对象 | `collect_garbage` | 分支 GC、篡改及故障注入测试 | 在单写者前提下已支持 | 定理显式列出完整 list 和哈希可靠性前提 |
| GC marking fail-closed | 可达对象的读取、哈希、类型、解析或路径验证失败时尚未开始删除 | mark/sweep 阶段顺序 | `marking_failure_happens_before_any_deletion` | 已支持 | 保留 |
| GC sweep 可重试但非事务 | 删除失败可能留下已删除的不可达对象；再次执行可完成清理 | 幂等 delete contract | `interrupted_sweep_is_safe_to_retry` | 已支持 | 删除 crash-atomicity 声明 |
| 分支路径不逃逸 refs | 所有入口验证名称，`ref_path` 验证直接父目录 | `validate_branch_name`、`ref_path` | path traversal 集成测试 | 已支持 | 保留 |
| 引用更新是原子替换 | 同目录临时文件经同步后由 `atomicwrites` 替换目标 | `atomic_write` | 正常路径测试；库行为由依赖及平台语义承担 | 有条件支持 | 仅声称引用的原子替换；不声称整个 commit 是事务 |
| 单写者互斥 | mutating operation 独占创建 `.chunklog/LOCK` | `RepositoryLock` | overlapping-writer 测试 | 已支持；进程崩溃可留下 stale lock | 在限制中明示 |
| 仓库格式可识别 | `.chunklog/FORMAT = 1`；缺失/未知版本均拒绝打开 | init/open | unsupported/missing-format 测试 | 已支持 | 旧实验格式无迁移命令，明确拒绝 |
| 历史存储增长 | 为唯一 Blob、Commit、Leaf 与受影响 Branch 的并集 | persistent radix Tree | `paper-results/structural-growth.md`，N=1,024、R=50、k=1/10/100 | 已支持并已测量 | 使用分类型实测和上界，删除旧闭式公式 |
| checkout/commit/load 绝对性能 | 固定主机、Rust 1.89、Criterion 0.8、唯一 256-byte payload | `benches/storage.rs` | `paper-results/benchmark-summary.md` 与 CSV | 已测量 | 同时报告正结果与 file-per-node 初始导入的负结果 |
| 真实引擎 payload 兼容性 | 官方 Luanti 5.16.1 生成 SQLite mapblock，按 `(x,z)` 聚合原始二进制 | `examples/luanti_workload.rs`、`paper-workloads/` | 2,023 mapblocks、289 columns，见归档结果 | 已支持（受控 singlenode） | 仅主张格式兼容和受控去重 |
| 生产玩家历史上的收益 | 需要真实 terrain/edit trace | 尚无 | 尚无 | 未验证，非本文主张 | 明列为外部有效性限制 |
| “首个完整实例化” | 需要严密检索范围及截止日期 | 不适用 | 现有相关工作足以反驳绝对措辞 | 不支持 | 已删除“首个”绝对主张 |

## 证据门槛

- **已支持**：实现、直接测试以及论文定义一致。
- **有条件支持**：正常路径成立，但依赖明确的平台、库或系统前提。
- **未验证**：实现或假设存在，但没有足够直接证据；不得作为结论。
- **不支持**：证据反驳该主张；论文必须删除或重写。

形式化结论必须来自明示的模型和证明；测试及实验只检查实现是否符合这些模型，并提供有限工作负载上的测量。
