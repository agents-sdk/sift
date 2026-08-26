// 预生成官网演示样本:调本地 @agent-context/sift 压缩 6 类真实感输入,
// 产出 site/src/data/samples.json(离线运行,站点本身零后端依赖)。
//
// 用法:node scripts/gen-demo-samples.mjs  (或 cd site && npm run gen:samples)
import { writeFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { siftText, detectContentType } from '../site/vendor/sift/dist/index.js';

const here = dirname(fileURLToPath(import.meta.url));

// ---------- 样本构造 ----------

function jsonArraySample() {
  const rows = [];
  const statuses = ['ok', 'ok', 'ok', 'degraded', 'ok', 'error'];
  for (let i = 1; i <= 48; i++) {
    rows.push({
      id: `svc-${String(i).padStart(3, '0')}`,
      name: `service-${i}`,
      region: ['us-east-1', 'us-west-2', 'eu-central-1', 'ap-southeast-1'][i % 4],
      status: statuses[i % statuses.length],
      latency_ms: 12 + ((i * 37) % 480),
      error_rate: (i % 7) * 0.15,
      replicas: 2 + (i % 5),
      version: `2.${i % 9}.${i % 13}`,
      last_deploy: `2026-08-${String(1 + (i % 21)).padStart(2, '0')}T0${i % 10}:15:00Z`,
      owner: ['team-alpha', 'team-beta', 'team-gamma'][i % 3],
    });
  }
  return JSON.stringify(rows, null, 2);
}

function buildOutputSample() {
  const lines = [
    '$ cargo build --release --workspace',
    '   Compiling libc v0.2.155',
    '   Compiling proc-macro2 v1.0.86',
    '   Compiling unicode-ident v1.0.12',
    '   Compiling quote v1.0.36',
    '   Compiling syn v2.0.68',
  ];
  for (let i = 0; i < 30; i++) {
    const crate = ['serde', 'serde_json', 'tokio', 'rayon', 'regex', 'anyhow', 'thiserror'][i % 7];
    lines.push(`   Compiling ${crate} v${1 + (i % 4)}.${i % 20}.${i % 9}`);
    if (i % 6 === 5) {
      lines.push(`warning: unused import: \`std::collections::HashMap\``);
      lines.push(` --> crates/sift/src/relevance.rs:${10 + i}:${5 + i}`);
      lines.push('  |');
      lines.push(`${10 + i} | use std::collections::HashMap;`);
      lines.push('  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^');
      lines.push('  |');
      lines.push('  = note: `#[warn(unused_imports)]` on by default');
      lines.push('  = help: remove this import');
    }
  }
  for (let i = 0; i < 12; i++) {
    lines.push(`warning: field \`legacy_${i}\` is never read`);
    lines.push(` --> crates/sift/src/policy.rs:${40 + i * 3}:9`);
    lines.push('  |');
    lines.push(`${40 + i * 3} |     legacy_${i}: Option<String>,`);
    lines.push('  |     ^^^^^^^^^^^^');
    lines.push('  |');
    lines.push('  = note: `#[warn(dead_code)]` on by default');
  }
  lines.push('   Compiling sift v0.4.2 (crates/sift)');
  lines.push('error[E0308]: mismatched types');
  lines.push('   --> crates/sift/src/tokenizer.rs:128:23');
  lines.push('    |');
  lines.push('128 |     let n: u32 = text.chars().count() as u64;');
  lines.push('    |                   ^^^^^^^^^^^^^^^^^^^^^^^^^ an `as` cast can silently truncate');
  lines.push('    |');
  lines.push('help: try: `text.chars().count().try_into().unwrap()`');
  lines.push('');
  lines.push('error: could not compile `sift` (bin "sift") due to 1 previous error');
  return lines.join('\n');
}

function searchResultsSample() {
  const files = [
    'crates/sift/src/transforms/mod.rs',
    'crates/sift/src/transforms/smart_crusher.rs',
    'crates/sift/src/transforms/log_compressor.rs',
    'crates/sift/src/stash.rs',
    'crates/sift/src/relevance.rs',
    'npm/core/src/index.ts',
    '.agents/PROJECT_MAP.md',
  ];
  const terms = ['siftText', 'StashStore', 'ReformatTransform', 'frozen', 'tokensSaved'];
  const lines = ['$ rg -n "siftText|StashStore|frozen" crates/ npm/ .agents/'];
  for (let i = 0; i < 60; i++) {
    const f = files[i % files.length];
    const t = terms[(i / 7) % terms.length | 0];
    lines.push(`${f}:${20 + i * 13}:${t} ${(t + ' ').repeat(i % 5)}related handler ${i}`);
  }
  return lines.join('\n');
}

function diffSample() {
  const hunks = [];
  for (let h = 0; h < 10; h++) {
    const start = 30 + h * 40;
    hunks.push(`@@ -${start},${12 + h} +${start},${10 + h} @@ fn transform_${h}(input: &str) -> String {`);
    for (let i = 0; i < 8 + h; i++) {
      hunks.push(` context line ${h}.${i} of the function body that stays the same`);
    }
    hunks.push('-    let old_impl = format!("{}-legacy", input);');
    hunks.push('-    return old_impl;');
    hunks.push('+    let new_impl = crush(input);');
    hunks.push('+    new_impl');
  }
  return ['diff --git a/src/lib.rs b/src/lib.rs', 'index 3f2a9c1..b7e4d02 100644', '--- a/src/lib.rs', '+++ b/src/lib.rs', ...hunks].join('\n');
}

// Java Spring Service:AST 感知压缩(imports/类/方法签名保留,长方法体折叠)
function sourceCodeSample() {
  const lines = [
    'package com.example.orders.service;',
    '',
    'import com.example.orders.model.Order;',
    'import com.example.orders.model.OrderStatus;',
    'import com.example.orders.repo.OrderRepository;',
    'import com.example.orders.exception.OrderNotFoundException;',
    'import org.springframework.stereotype.Service;',
    'import org.springframework.transaction.annotation.Transactional;',
    'import java.math.BigDecimal;',
    'import java.time.Instant;',
    'import java.util.List;',
    'import java.util.stream.Collectors;',
    '',
    '/** 订单服务:负责订单生命周期管理与查询。 */',
    '@Service',
    'public class OrderService {',
    '',
    '    private final OrderRepository orderRepository;',
    '    private final PaymentGateway paymentGateway;',
    '    private final NotificationClient notificationClient;',
    '',
    '    public OrderService(OrderRepository orderRepository,',
    '                        PaymentGateway paymentGateway,',
    '                        NotificationClient notificationClient) {',
    '        this.orderRepository = orderRepository;',
    '        this.paymentGateway = paymentGateway;',
    '        this.notificationClient = notificationClient;',
    '    }',
    '',
    '    @Transactional',
    '    public Order createOrder(CreateOrderRequest request) {',
    '        validate(request);',
    '        Order order = new Order();',
    '        order.setUserId(request.getUserId());',
    '        order.setItems(request.getItems());',
    '        order.setAmount(request.getItems().stream()',
    '                .map(i -> i.getPrice().multiply(BigDecimal.valueOf(i.getQuantity())))',
    '                .reduce(BigDecimal.ZERO, BigDecimal::add));',
    '        order.setStatus(OrderStatus.PENDING);',
    '        order.setCreatedAt(Instant.now());',
    '        Order saved = orderRepository.save(order);',
    '        try {',
    '            PaymentResult payment = paymentGateway.charge(saved.getAmount(), saved.getUserId());',
    '            if (!payment.isSuccess()) {',
    '                saved.setStatus(OrderStatus.PAYMENT_FAILED);',
    '                orderRepository.save(saved);',
    '                notificationClient.notify(saved.getUserId(), "支付失败,订单已保留");',
    '                return saved;',
    '            }',
    '            saved.setStatus(OrderStatus.PAID);',
    '            saved.setPaymentId(payment.getPaymentId());',
    '            orderRepository.save(saved);',
    '        } catch (PaymentGatewayException e) {',
    '            saved.setStatus(OrderStatus.PAYMENT_ERROR);',
    '            orderRepository.save(saved);',
    '            throw e;',
    '        }',
    '        notificationClient.notify(saved.getUserId(), "下单成功");',
    '        auditLog.record("order.created", saved.getId());',
    '        metrics.increment("orders.created");',
    '        return saved;',
    '    }',
    '',
    '    @Transactional(readOnly = true)',
    '    public Order findById(Long id) {',
    '        return orderRepository.findById(id)',
    '                .orElseThrow(() -> new OrderNotFoundException(id));',
    '    }',
    '',
    '    @Transactional(readOnly = true)',
    '    public List<OrderSummary> listByUser(Long userId, int page, int size) {',
    '        return orderRepository.findByUserIdOrderByCreatedAtDesc(userId)',
    '                .stream()',
    '                .skip((long) page * size)',
    '                .limit(size)',
    '                .map(OrderSummary::from)',
    '                .collect(Collectors.toList());',
    '    }',
    '',
    '    @Transactional',
    '    public void cancelOrder(Long id, String reason) {',
    '        Order order = findById(id);',
    '        if (order.getStatus() == OrderStatus.SHIPPED) {',
    '            throw new IllegalStateException("已发货订单不可取消");',
    '        }',
    '        if (order.getStatus() == OrderStatus.CANCELLED) {',
    '            return;',
    '        }',
    '        order.setStatus(OrderStatus.CANCELLED);',
    '        order.setCancelReason(reason);',
    '        order.setCancelledAt(Instant.now());',
    '        orderRepository.save(order);',
    '        if (order.getPaymentId() != null) {',
    '            paymentGateway.refund(order.getPaymentId(), order.getAmount());',
    '        }',
    '        notificationClient.notify(order.getUserId(), "订单已取消: " + reason);',
    '        auditLog.record("order.cancelled", id);',
    '    }',
    '',
    '    @Transactional',
    '    public Order shipOrder(Long id, String trackingNo) {',
    '        Order order = findById(id);',
    '        if (order.getStatus() != OrderStatus.PAID) {',
    '            throw new IllegalStateException("仅已支付订单可发货");',
    '        }',
    '        order.setStatus(OrderStatus.SHIPPED);',
    '        order.setTrackingNo(trackingNo);',
    '        order.setShippedAt(Instant.now());',
    '        Order saved = orderRepository.save(order);',
    '        notificationClient.notify(saved.getUserId(), "已发货,单号 " + trackingNo);',
    '        metrics.increment("orders.shipped");',
    '        return saved;',
    '    }',
    '',
    '    private void validate(CreateOrderRequest request) {',
    '        if (request.getUserId() == null) {',
    '            throw new IllegalArgumentException("userId 不能为空");',
    '        }',
    '        if (request.getItems() == null || request.getItems().isEmpty()) {',
    '            throw new IllegalArgumentException("订单项不能为空");',
    '        }',
    '        for (Item item : request.getItems()) {',
    '            if (item.getQuantity() <= 0) {',
    '                throw new IllegalArgumentException("数量必须为正数");',
    '            }',
    '        }',
    '    }',
    '}',
  ];
  return lines.join('\n');
}


function plainTextSample() {
  const speakers = ['Alice', 'Bob', 'Carol', 'Dave'];
  const out = ['周会纪要:上下文压缩项目(2026-08-18)', ''];
  for (let i = 0; i < 26; i++) {
    const s = speakers[i % 4];
    const topic = ['冻结前缀', 'CCR 恢复', 'token 估算', '发布节奏', '文档', '压测'][i % 6];
    out.push(`${s}:关于${topic},我觉得目前方案整体可以,细节上还有几个点需要确认一下,第一是边界条件,第二是回退路径,第三是和缓存成本的折中。`);
    out.push(`${s}:另外上次的 action item 我这边已经完成了,详情见 ticket COMP-${100 + i}。`);
    if (i % 5 === 4) out.push(`${s}:这个问题我们下次会议再深入讨论吧,先把结论记下来:${topic}维持现状。`);
  }
  out.push('', '结论:1) 冻结前缀下界算法保持不变;2) CCR store 换成内存 LRU;3) 下周二发布 0.5.0。');
  return out.join('\n');
}

// ---------- 元数据 ----------

const SAMPLES = [
  { type: 'json_array', label: 'JSON 数组', query: 'error degraded service status', desc: '结构化监控数据(schema 去重 + 采样,关键行保留)', make: jsonArraySample },
  { type: 'build_output', label: '构建日志', query: 'error mismatched types could not compile', desc: 'cargo/npm 构建输出(错误与堆栈保留,重复 warning 折叠)', make: buildOutputSample },
  { type: 'search_results', label: '搜索结果', query: 'siftText StashStore frozen', desc: 'grep / ripgrep 输出(重复行抽稀,匹配项保留)', make: searchResultsSample },
  { type: 'git_diff', label: 'Git Diff', query: 'new_impl crush', desc: 'unified diff(hunk 采样,改动行保留)', make: diffSample },
  { type: 'source_code', label: 'Java 源码', query: 'cancel refund 支付', desc: 'AST 感知代码压缩(imports/签名/类型保留,长方法体折叠)', make: sourceCodeSample },
  { type: 'plain_text', label: '纯文本', query: '结论 冻结前缀 CCR 发布', desc: '中英文抽取式摘要(BM25 相关性 + 近重复折叠)', make: plainTextSample },
];

// ---------- 生成 ----------

const results = [];
for (const s of SAMPLES) {
  const original = s.make();
  const detected = detectContentType(original);
  const r = siftText(original, s.query);
  results.push({
    type: s.type,
    label: s.label,
    desc: s.desc,
    query: s.query,
    detected,
    changed: r.changed,
    lossy: r.lossy,
    stashKey: r.stashKey,
    tokensSaved: r.tokensSaved,
    original,
    compressed: r.text,
  });
  console.log(
    `${s.label}: detected=${detected} changed=${r.changed} lossy=${r.lossy} saved=${r.tokensSaved} ` +
      `${original.length}B -> ${r.text.length}B`,
  );
}

const outDir = join(here, '..', 'site', 'src', 'data');
mkdirSync(outDir, { recursive: true });
writeFileSync(join(outDir, 'samples.json'), JSON.stringify(results));
console.log(`\n已写入 ${outDir}/samples.json`);
