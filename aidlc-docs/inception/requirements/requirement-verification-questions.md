# Requirements Verification Questions

`draft-spec.md` の内容を踏まえて、要件を確定するための質問です。各質問の [Answer]: の後に選択肢のアルファベットを記入してください。選択肢が合わない場合は最後の「Other」を選び、内容を記述してください。

---

## A. プロジェクトの位置づけ・命名

### Question A1
draft-spec.md に記載の命名懸念(crates.io未確認、GitHub組織`neatnik`の存在、類似ツール`neatcli`)について、現時点でどう扱いますか？

A) 開発中は仮称`neatnik`のまま進め、公開直前に正式名称・リポジトリ名を再確認する

B) 今すぐ別名を決めて開発初期から使う(別名の候補があれば[Answer]に記載)

C) 命名は本要件定義のスコープ外とし、後日別途検討する

D) Other (please describe after [Answer]: tag below)

[Answer]: A

### Question A2
初回リリース(MVP)のスコープはどこまでを想定しますか？

A) 3段階(アーカイブ→退避→削除)すべてをフル実装してリリース

B) まず「削除」機能のみ(最もシンプルで需要が高い)を実装し、段階的にアーカイブ・退避を追加

C) まず「アーカイブ→削除」の2段階(退避は後回し)を実装

D) Other (please describe after [Answer]: tag below)

[Answer]: A

---

## B. アーカイブ機能

### Question B1
圧縮形式について、初期実装の範囲をどうしますか？(提案: 依存クレート削減のため絞り込み)

A) 提案通り、初期は「単体ファイル→gzip」「バンドル→tar.gz」のみとし、zipは将来拡張とする

B) draft-spec.md通り、gzip/zip/tar.gzすべてを初期実装に含める

C) Other (please describe after [Answer]: tag below)

[Answer]: B

### Question B2
圧縮処理のアトミック性(処理中断時に中途半端なアーカイブファイルを残さない)について、一時ファイル書き込み→成功時リネーム方式を採用しますか？

A) 採用する(提案通り)

B) 不要(現状のシンプルな実装で十分)

C) Other (please describe after [Answer]: tag below)

[Answer]: A

---

## C. 実行制御・安全性

### Question C1
「削除件数・容量が閾値超過時に自動実行を止める」セーフティブレーキについて、cron等の無人実行では対話的な確認ができません。どう挙動させますか？

A) 提案通り、閾値超過時は処理を中断・通知し、ロックファイル等で次回実行も自動的に止め続け、人手でロック解除するまで再開しない

B) 閾値超過時はその回の削除のみスキップ(ログ・通知は行うが、次回実行はブロックしない)

C) 閾値超過の判定自体は行うが、実際に止めるかどうかは設定でOn/Off選択できるようにする

D) Other (please describe after [Answer]: tag below)

[Answer]: C

### Question C2
同一ジョブの多重起動防止ロック機構は、どの方式を想定しますか？

A) OSのアドバイザリファイルロック(flock等)を使う

B) PIDファイル方式(存在チェック+プロセス生存確認)を使う

C) 実装方式はAIに一任する

D) Other (please describe after [Answer]: tag below)

[Answer]: A(`fd-lock`等のクロスプラットフォーム対応クレートで実装し、OS分岐はクレート内部に閉じ込める)

### Question C3
「ロック中ファイル(他プロセスが書き込み中)は除外する」とありますが、判定方法についてどう考えますか？

A) OSのアドバイザリロック(flock)が取得されているファイルのみ除外対象と判定する(ロックを使っていないプロセスの書き込み中ファイルは検出できない前提を許容)

B) より単純に、直近の更新時刻が極端に新しい(例: 数秒以内)ファイルは書き込み中とみなして除外する

C) 両方を組み合わせる

D) Other (please describe after [Answer]: tag below)

[Answer]: D(OSごとに検出戦略を分ける。Linux/macOSはC(flock検知+直近更新時刻ヒューリスティック、ベストエフォート)、Windowsは共有モードでのオープン試行によるERROR_SHARING_VIOLATION検出。オープン試行はファイル内容・タイムスタンプを変更しない)

### Question C4
`run`コマンド実行時に、`validate`相当の設定検証(N1<N2<N3等)を毎回自動的に内部で行い、不正な設定なら実行前に中断する、という提案についてどうしますか？

A) 採用する(`run`は常に内部バリデーションを通す)

B) 不要(ユーザーが事前に`validate`を実行する運用を徹底する)

C) Other (please describe after [Answer]: tag below)

[Answer]: A

### Question C5
各ステージが独立に無効化できる場合(例: アーカイブをスキップして退避のみ有効)、猶予日数の大小関係バリデーション(N1<N2<N3)はどう扱いますか？

A) 有効なステージ同士のみ比較する(例: アーカイブ無効なら退避のN2と削除のN3の大小関係だけ検証)

B) 無効化されていても設定値が入力されていれば常に全体の大小関係を検証する

C) Other (please describe after [Answer]: tag below)

[Answer]: A

---

## D. 冪等性・ログ・その他運用面

### Question D1
「同じジョブを複数回実行しても二重処理されない」冪等性は、どう実現しますか？

A) 移動先/圧縮先に同名(または対応する)ファイルが既に存在するかどうかで判定する(シンプルだが命名規則の衝突に注意)

B) 処理済みファイルの記録(状態ファイルやメタデータ)を別途保持して判定する

C) Other (please describe after [Answer]: tag below)

[Answer]: A

### Question D2
ツール自身が出力するログファイルが、監視対象ディレクトリと重ならないようにする対応についてどうしますか？

A) デフォルトでツール自身のログ出力先を除外パターンに自動追加する

B) 特に対応不要(ユーザーが除外パターンで自己管理する)

C) Other (please describe after [Answer]: tag below)

[Answer]: B(暗黙の自動除外は「指定していない構成で動く」ことによる混乱・ミスリードを生むため不採用)

### Question D3
非機能要件の「パフォーマンス：大量ファイル(数万件規模)でも実用的な時間で走査・処理できること」について、具体的な目標値はありますか？

A) 具体的な数値目標はなく、「実用上ストレスのない体感速度」で問題ない

B) 具体的な目標がある([Answer]に記載してください。例: 10万ファイルを5分以内)

C) Other (please describe after [Answer]: tag below)

[Answer]: A

### Question D4
対象OS・実行環境はどこまでサポートしますか？

A) Linuxサーバのみ(cron前提)

B) Linux/macOS(開発機での利用も想定)

C) Linux/macOS/Windows

D) Other (please describe after [Answer]: tag below)

[Answer]: C(OSによる制限事項・実装差異が生じることは許容する)

### Question D5
通知機能(エラー時・削除件数閾値超過時にメール/Slack等)は、初回リリースのスコープに含めますか？

A) 初回リリースでは実装せず、通知用のtrait/インターフェースだけ用意して将来拡張しやすくしておく

B) 初回リリースからSlack通知(Webhook)程度は実装する

C) 初回リリースには一切含めない(traitの用意も不要)

D) Other (please describe after [Answer]: tag below)

[Answer]: A

---

## E. 拡張機能 Opt-In

### Question: Security Extensions
Should security extension rules be enforced for this project?

A) Yes — enforce all SECURITY rules as blocking constraints (recommended for production-grade applications)

B) No — skip all SECURITY rules (suitable for PoCs, prototypes, and experimental projects)

X) Other (please describe after [Answer]: tag below)

[Answer]: A

### Question: Property-Based Testing Extension
Should property-based testing (PBT) rules be enforced for this project?

A) Yes — enforce all PBT rules as blocking constraints (recommended for projects with business logic, data transformations, serialization, or stateful components)

B) Partial — enforce PBT rules only for pure functions and serialization round-trips (suitable for projects with limited algorithmic complexity)

C) No — skip all PBT rules (suitable for simple CRUD applications, UI-only projects, or thin integration layers with no significant business logic)

X) Other (please describe after [Answer]: tag below)

[Answer]: B

### Question: Resiliency Extensions
Should the resiliency baseline be applied to this project?

**What this extension is.** Enabling it applies a set of **directional, design-time best practices** for building resilient systems, derived from the **AWS Well-Architected Framework (Reliability Pillar)** and resilience-review guidance. It steers requirements, design, and code toward fault tolerance, high availability, observability, and recoverability — covering 15 practice areas across business goals, change management, observability, high availability, disaster recovery, and continuous improvement.

**What this extension is NOT.** Enabling it does **not** make your workload production-ready, nor does it certify or guarantee any availability, RTO, or RPO target. It is a **starting point** that scaffolds good resiliency decisions early — it is not a substitute for a formal **AWS Well-Architected Review** of the built system.

A) Yes — apply the resiliency baseline as directional best practices and design-time guidance (recommended for business-critical workloads, as an informed starting point that you can validate and harden before go-live)

B) No — skip the resiliency baseline (suitable for PoCs, prototypes, and experimental projects where rapid iteration matters more than reliability)

X) Other (please describe after [Answer]: tag below)

[Answer]: B
