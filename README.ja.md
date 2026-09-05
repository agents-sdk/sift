# sift

**送信するコンテキストを減らし、原文にはいつでも戻れるように。**

sift は、大きなツール出力を LLM に送信する前に圧縮します。トークン使用量とプロンプトキャッシュのコストを抑えながら、非可逆圧縮された原文をローカル stash から復元できます。圧縮エンジンは Rust 製で、Node.js 向けに [`@agent-context/sift`](npm/core/README.md) として提供されています。

[English](README.md) · [简体中文](README.zh-CN.md) · 日本語 · [Español](README.es.md)

### **コンテキストを 52.6% 削減。推定 14,016 トークンを節約。非可逆ベンチマークはすべて復元成功。**

12 個の内蔵ベンチマーク全体で **88,759 B から 42,037 B** へ削減し、**非可逆 8/8 ケースで原文の復元に成功**しました。[測定結果の詳細を見る。](#どれくらい削減できるか)

```sh
npm install @agent-context/sift
```

ステータス：**Alpha** · 1.0 までは API の詳細が変わる可能性があります · [運用上の注意](#運用上の注意)

## すぐに使えるエージェント統合

[Pi](https://github.com/earendil-works/pi) または [OpenCode](https://github.com/anomalyco/opencode) を利用している場合は、対応するアダプターをそのままインストールできます。新しいツール出力を自動的に圧縮し、必要に応じて stash の原文を取得する `sift_retrieve` ツールをエージェントに登録します：

- **Pi：** `pi install npm:@agent-context/pi-sift`
- **OpenCode：** `opencode.json` の `plugin` 配列に `["@agent-context/opencode-sift", { "minLength": 200 }]` を追加

インストール、設定、ストレージ、トラブルシューティングの詳細は [agents-sdk/sift-plugins](https://github.com/agents-sdk/sift-plugins) を参照してください。

## sift を使う理由

エージェントの会話は、ビルドログ、検索結果、diff、ソースコード、JSON レスポンスによって急速に大きくなります。しかし、その中で次の推論に本当に必要な情報は一部だけです。毎ターン全文を送り直すとトークンを消費し、重要なコンテキストの余地も減ります。

sift が提供するもの：

- **コンテキストコストの削減** — 内蔵ベンチマークでは 88,759 バイトから 42,037 バイトへ、**52.6% 削減**しました。
- **重要な情報を優先** — エラー、スタックトレース、コマンド、関連する検索結果、構造情報を見える状態に保ちます。
- **復元可能な圧縮** — 非可逆な結果を返す前に原文全体を保存し、`<<stash:HASH>>` マーカーを付けます。
- **プロンプトキャッシュを保護** — Anthropic の `cache_control` アンカー以前のメッセージには触れません。
- **1 か所で統合** — Anthropic Messages、OpenAI Chat Completions、OpenAI Responses を自動判定します。
- **Rust コアとシンプルな API** — コンパイルされた圧縮ロジックを小さな Node.js API から利用できます。

### 単純な切り捨てより有用で、一方向の要約より安全

| 方法 | 内容を認識 | 原文を復元 | Anthropic キャッシュ接頭辞を保護 | 効果がない結果を拒否 |
| --- | :---: | :---: | :---: | :---: |
| 単純な切り捨て | いいえ | いいえ | 必ずしも | いいえ |
| LLM 要約 | 一部 | 通常不可 | 必ずしも | いいえ |
| **sift** | **はい** | **はい** | **はい** | **はい** |

## どれくらい削減できるか

リポジトリ内の固定された 12 個の [demo 入力](npm/core/demo/cases)を、現在のソースツリー（パッケージバージョン `0.0.1-alpha.7`）で測定した結果です。測定方法と再現手順は [BENCHMARK.md](BENCHMARK.md) を参照してください：

| シナリオ | 入力 | 出力 | サイズ削減 | 推定節約トークン | 復元 |
| --- | ---: | ---: | ---: | ---: | --- |
| JSON 配列 | 18,397 B | 12,448 B | 32.3% | 1,785 | ロスレス |
| Pretty JSON | 3,642 B | 973 B | 73.3% | 801 | ロスレス |
| ビルドログ | 3,073 B | 1,543 B | 49.8% | 459 | ロスレス |
| 検索結果 | 10,057 B | 3,227 B | 67.9% | 2,049 | PASS |
| Git diff | 23,007 B | 7,795 B | 66.1% | 4,564 | PASS |
| 混合コマンド出力 | 9,240 B | 3,879 B | 58.0% | 1,608 | ロスレス |
| Rust ソースコード | 2,282 B | 572 B | 74.9% | 513 | PASS |
| 繰り返しプレーンテキスト | 2,723 B | 454 B | 83.3% | 680 | PASS |
| 固有情報と機密値の保護 | 3,125 B | 1,540 B | 50.7% | 476 | PASS |
| HTML 本文抽出 | 1,036 B | 337 B | 67.5% | 209 | PASS |
| 構造化設定 | 2,698 B | 1,994 B | 26.1% | 211 | PASS |
| Markdown テーブル | 9,479 B | 7,275 B | 23.3% | 661 | PASS |
| **合計** | **88,759 B** | **42,037 B** | **52.6%** | **14,016** | **非可逆 8/8 件を復元** |

これは公開 fixture の透明な測定結果であり、すべてのワークロードに対する保証ではありません。認証情報らしい値は可視出力に残しつつ、その他の低価値な内容を圧縮できます。すべての非可逆ケースで完全な原文を復元できます。`tokensSaved` は sift 内蔵の推定値です。

## クイックスタート

```sh
npm install @agent-context/sift
```

LLM リクエストを送信する直前に圧縮します：

```ts
import OpenAI from "openai";
import { siftRequest } from "@agent-context/sift";

const openai = new OpenAI();
const request = {
  model: "gpt-5.6-sol",
  input: conversationWithLargeToolOutputs,
};

const result = siftRequest(request, currentUserQuestion);
const response = await openai.responses.create(result.body as any);

console.log({
  changed: result.changed,
  tokensSaved: result.tokensSaved,
  blocksCompressed: result.blocksCompressed,
});
```

`siftRequest` が変更するのは対象となるツール出力だけです。system、user、assistant のプロンプトはデフォルトで保護されます。

単独のツール結果やファイルを圧縮する場合：

```ts
import { siftText } from "@agent-context/sift";

const result = siftText(
  fileContents,
  currentUserQuestion,
  "src/services/OrderService.java", // 任意：言語判定を安定させます
);

console.log(result.text);
console.log(result.tokensSaved);
```

512 バイト未満の入力はそのまま返されるため、各ブロックを事前選別せず一般的なリクエスト経路に組み込めます。

### モデルに見える内容

以下は効果のイメージです。何百、何千行もの反復を次のターンへ持ち越さず、重要な構造と原文へ戻る経路を残します：

```diff
- 2,000 行のコマンド、反復ステータス、スタックトレース
+ $ cargo test --workspace
+ error[E0382]: borrow of moved value: `request`
+   --> src/client.rs:84:17
+ [... 1,962 lines omitted from file "/home/agent/.sift/stash/HASH", starting at line 19]
+ test result: FAILED. 127 passed; 1 failed
+ <<stash:HASH>>
```

エラーと要約は表示されたままです。省略行は stash マーカーまたは正確なファイル範囲から取得できます。

## 原文を復元する

非可逆圧縮では、結果を返す前に入力全体が必ず stash に保存されます。出力には次のようなマーカーが付きます：

```text
<<stash:8f1c2e...>>
```

原文全体、または必要な行だけを取得できます：

```ts
import { retrieve, retrieveLines, siftText } from "@agent-context/sift";

const result = siftText(longToolOutput, currentUserQuestion);

if (result.stashKey) {
  const original = retrieve(result.stashKey);
  const slice = retrieveLines(result.stashKey, 120, 80);
}
```

ソースコード、ログ、検索結果、diff、行単位のプレーンテキストでは、省略箇所から stash ファイルと正確な行範囲を直接参照できます：

```text
// ... 30 lines omitted from file "/home/agent/.sift/stash/HASH", starting at line 32
```

同じファイルシステムを使うエージェントは、その範囲を直接読めます。それ以外では、アプリケーションや独自ツールから `retrieve` / `retrieveLines` を公開してください。sift 自体はモデルへ取得ツールを自動注入しません。

## 内容に応じた圧縮

| 入力 | sift が保持・簡略化する内容 |
| --- | --- |
| JSON 配列 | 均一なレコードは CSV-schema に変換し、異種レコードは schema buckets に分割可能。どちらも全行を保持し、256 B を超える opaque セルは個別に復元可能な stash へ移動。残りの構造だけを再帰的にサンプリング |
| ビルド・テストログ | コマンド、エラー、スタックトレース、要約 |
| grep / ripgrep 結果 | ソースの文脈とともに整理された有用な一致 |
| Unified diff | 代表的な hunk と変更構造 |
| ソースコード | シグネチャ、構造、完全な AST 文の先頭 5 行を保持してから関数本体を折りたたむ。Python、JavaScript、TypeScript、Go、Rust、Java、C、C++ に対応 |
| プレーンテキスト | query 関連性、位置、顕著性に基づく抽出的選択と近重複の抑制 |
| YAML、TOML、INI 設定 | すべてのキー、値、順序を保持し、安全な行全体コメントと空行のみを stash へ移動 |
| CSV、TSV、Markdown テーブル | 列構造を厳密に解析して CSV-schema に渡し、採用時は全レコードを保持。すでに十分コンパクトな入力はそのまま返す |
| 整形済み JSON・反復ログ | 可能ならロスレスな minify またはテンプレート化 |

HTML は記事本文を読みやすい Markdown として抽出し、スクリプト、スタイル、ナビゲーション、サイドバー、広告、フッターを除去します。原文全体は stash から復元できます。

## 安全に導入するための設計

sift には 3 つの不変ルールがあります：

1. 圧縮は各メッセージ内だけで行い、会話からメッセージ全体を削除しません。
2. Anthropic の `cache_control` アンカー以前にある凍結プレフィックスを変更しません。
3. 非可逆変換では、圧縮結果を返す前に必ず原文の保存を完了します。

さらに、ツール呼び出しと結果の対応、カスタム XML タグ、認証情報の可能性がある高エントロピー文字列を保護します。トークンが減らない場合や stash への書き込みに失敗した場合は原文を返します。

## どこに組み込むか

`siftRequest` は、LLM への送信直前に実行される最後のミドルウェアとして配置するのがおすすめです。特に次の用途に向いています：

- ビルド出力、検索結果、diff を何度も保持するコーディングエージェント
- 大きなツールレスポンスを扱う長時間のアシスタント
- Anthropic と OpenAI の両形式を扱うゲートウェイ
- 省略した詳細をモデルが後から要求できるローカル／サーバーワークフロー

完全なリクエストではなく 1 つの文字列を扱う場合は `siftText` を使います。

## API 一覧

```ts
siftRequest(body, query?)
siftText(text, query?, sourcePath?)
retrieve(key)
retrieveLines(key, startLine, lineCount)
createSift({ stashDir })
detectContentType(text)
detectRequestFormat(body)
```

戻り値の型、リクエスト形式、詳しい挙動は [Node.js パッケージのドキュメント](npm/core/README.md)を参照してください。

## 運用上の注意

- デフォルトの stash は `~/.sift/stash` です。`SIFT_STASH_DIR` または `createSift({ stashDir })` で変更できます。
- stash エントリは 30 分後に期限切れとなり、読み取り時に遅延削除されます。復元と保持の設計に注意してください。
- ローカル stash は同一マシンのプロセス間では共有できますが、クラスタには自動共有されません。複数ホストでは共有ファイルシステムまたは共有 `StashStore` バックエンドが必要です。
- `tokensSaved` は観測用の推定値であり、請求照合用ではありません。
- Node.js パッケージには macOS、Linux（GNU / musl）、Windows 向けの x64 / arm64 バイナリが含まれます。Linux GNU ビルドの基準は glibc 2.28 です。

## コントリビューション

ビルド手順、アーキテクチャのルール、テスト要件、リリースフローは [CONTRIBUTING.md](CONTRIBUTING.md) にまとめています。

## ライセンス

[Apache-2.0](LICENSE)
