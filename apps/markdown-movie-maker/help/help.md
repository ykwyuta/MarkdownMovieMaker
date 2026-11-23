# MarkdownMovieMaker ヘルプ

MarkdownMovieMaker は、Markdown 風の台本から、音声合成・立ち絵・字幕・BGM 付きの解説動画（MP4）を作る
Windows アプリケーションです。台本を書くとすぐにスライド形式でプレビューでき、シーンごとに再生して確かめながら
仕上げられます。Python・ffmpeg・ImageMagick は不要です。

**目次**

- [はじめに](#start)
- [画面の見方](#screen)
- [スライド（シーン一覧）](#slides)
- [プレビューと再生](#preview)
- [ノート（シーンのセリフ）](#notes)
- [タイムライン](#timeline)
- [台本エディタ](#editor)
- [メッセージ](#messages)
- [音声合成](#voice)
- [書き出し](#export)
- [台本の書き方](#syntax)
- [YMM4 の機能との対応](#ymm4)
- [シーンの設定](#scene)
- [場面切り替え](#transition)
- [アイテム共通の属性](#props)
- [時刻（at / until）](#time)
- [アニメーション（中間点）](#keyframes)
- [表示・非表示アニメーション](#motion)
- [ぱりーんと割れる](#shatter)
- [映像エフェクト](#fx)
- [アイテム（テキスト・画像・動画・図形）](#items)
- [立ち絵](#character)
- [セリフ](#lines)
- [BGM・効果音](#audio)
- [キャラクター定義](#master)
- [コマンドライン（mmm）](#cli)
- [キーボードショートカット](#shortcuts)
- [困ったときは](#trouble)

## はじめに {#start}

1. 「ファイル → 開く」(Ctrl+O) で台本（`.md`）を開きます。台本と同じフォルダーか、その上のフォルダーにある
   `data/character_master.json` の場所が素材の基準になります（ステータスバーに表示）。
2. 左のスライドでシーンを選び、「▶ このシーン」(F5) で再生して確かめます。
3. 「音声を合成」(F6) でセリフの声を作ります。合成するまでは推定の長さで無音のままプレビューします。
4. 「書き出し…」(Ctrl+E) で MP4 などに書き出します。

新しく書くときは「ファイル → 新規」(Ctrl+N) でひな形から始められます。

## 画面の見方 {#screen}

PowerPoint のように、シーンをスライドとして扱います。

| 場所 | 内容 |
| --- | --- |
| 上 | 音声を合成・このセリフ・書き出し・中止、音声エンジンの選択、処理の状態 |
| 左 | [スライド](#slides)（シーンのサムネイル） |
| 中央 | [プレビュー](#preview)、再生の操作、[ノート](#notes)（このシーンのセリフ）、[タイムライン](#timeline) |
| 右 | [台本エディタ](#editor)と[メッセージ](#messages) |
| 下 | ステータスバー（素材の基準フォルダー） |

## スライド（シーン一覧） {#slides}

シーンごとにサムネイル、番号と名前、開始時刻・長さ・セリフの数（うち合成済みの数）を表示します。

- クリックでシーンを選び、プレビューと台本がそのシーンへ移ります。
- ダブルクリック、Enter、Space でそのシーンだけを再生します。
- ↑ / ↓、Ctrl+PageUp / Ctrl+PageDown で前後のシーンへ移ります。
- セリフも間もないシーンは動画に含まれず「（表示なし）」になります。
- 台本を書き換えると、内容の変わったシーンのサムネイルだけを描き直します。

## プレビューと再生 {#preview}

選んだシーンを大きく表示します。下の操作で再生します。

| 操作 | 内容 |
| --- | --- |
| ◀ 前 / 次 ▶ | 前後のシーン |
| ▶ このシーン (F5) | シーンの頭から終わりまで再生して止まる（最後の絵が残る） |
| ▶ 通して再生 (Shift+F5) | 今の位置から最後まで再生する |
| プレビューをクリック | このシーンの再生・停止 |

時刻の欄には動画全体の位置と、シーン内の位置を表示します。音声出力がない環境では映像だけを再生します。

## ノート（シーンのセリフ） {#notes}

選んだシーンのセリフの一覧です。行頭の印は合成の状態です。

| 印 | 意味 |
| --- | --- |
| `[済]` | 合成済み |
| `[推定]` | まだ合成していない（推定の長さでプレビュー） |
| `[失敗]` | 合成に失敗した（[メッセージ](#messages)に理由） |
| `[未定義]` | キャラクター定義にない話者（動画では飛ばす） |

クリックするとそのセリフへ移り、ダブルクリックでそのセリフだけを合成してシーンの終わりまで再生します。

## タイムライン {#timeline}

YMM4 のタイムラインのように、上から「シーン・セリフ・立ち絵・アイテム・音声」の帯を並べ、赤い線で今の位置を示します。

- クリック・ドラッグで移動します。
- ホイールで拡大縮小、Shift+ホイールで左右に送ります。
- 帯をダブルクリックすると、台本のその行へ移ります。

## 台本エディタ {#editor}

yyeditor と同じエディタ部品です。日本語入力は文中に直接表示されます。

- 入力が止まって約 0.3 秒で、スライド・プレビュー・タイムラインに反映されます。
- キャレットのある行のシーン・セリフへプレビューが移ります。
- F1 で[台本の書き方](#syntax)を開きます。
- Ctrl+S で保存します。保存していない変更があると、閉じる・開く・新規のときに確認します。

## メッセージ {#messages}

台本の警告（「n 行目: …」）、読めない素材、処理の結果を表示します。警告の行をダブルクリックすると台本のその行へ移ります。

## 音声合成 {#voice}

既定のエンジンは **VOICEVOX**（起動済みの VOICEVOX エンジンを使用）です。
上のエンジンの選択で、すべてのキャラクターを別のエンジン（Kokoro-82M や無音）にできます。

| エンジン | 内容 |
| --- | --- |
| VOICEVOX（既定） | 起動済みの VOICEVOX エンジンを使う |
| Kokoro-82M | 軽量・オフライン。CPU でも速い（初回はモデルをダウンロード） |
| 無音 | 推定の長さの無音（タイミングの確認用） |

- 「音声を合成」(F6) はまだ合成していないセリフをすべて合成します。「このセリフ」(F7) はキャレットのあるセリフだけを合成して再生します。
- 「プロジェクト → 声を試聴」でキャラクターの声を確かめられます。
- 「設定 → 音声合成のデバイス」で `auto`（GPU があれば GPU）・`cuda:0`・`cpu` を選びます。GPU で推論するには CUDA 対応版のビルドが必要です。
- 合成した音声はキャッシュに保存し、同じセリフは合成し直しません（環境変数 `MMM_CACHE` で場所を変えられます）。
- 読みの辞書 `data/voice_dictionary.csv`（`表記,よみ`）も使われます。

## 書き出し {#export}

「書き出し…」(Ctrl+E) で保存先を選びます。拡張子で種類が決まります。

| 拡張子 | 内容 |
| --- | --- |
| `.mp4` | 動画（H.264） |
| `.mp3` / `.wav` | 音声だけ |
| `.png` | セリフごとの静止画（連番） |

「設定」で解像度（1920×1080・1280×720・3840×2160）、フレームレート、動画エンコーダを選びます。
既定のエンコーダは Windows 標準の Media Foundation（GPU のハードウェアエンコーダ、音声は AAC）です。
「純 Rust」を選ぶと Media Foundation を使わずに書き出します（音声は Opus）。
未合成のセリフは書き出しのときに合成します。書き出し中は「中止」で止められます。

## 台本の書き方 {#syntax}

台本は `# Scene` でシーンを区切り、その下に設定、`## Lines` の下にセリフを書きます。
設定の行・セリフの行の末尾には属性 `{キー: 値, …}` を書けます。

```markdown
# Scene@オープニング

- bg: data/bg/Blackboard.png {scale: 1.15->1, fx: blur(6)}
- transition: wipe-left 0.6
- length: 3
- text: "タイトル" {size: 110, color: #ffe000, outline: 6 #000 | 12 #fff, in: pop 0.6}
- id: zundamon@default_left {in: slide-left 0.5}

## Lines
zundamon: ｜詐欺《さぎ》の話なのだ！ {face: smile, pitch: 1.1, motion: bounce(30), wait: 0.5}
```

解釈できない行・未知のキー・不正な値は[メッセージ](#messages)に警告として表示し、動画の作成は続けます。

## YMM4 の機能との対応 {#ymm4}

| YMM4 | MarkdownMovieMaker |
| --- | --- |
| ボイスアイテム（セリフ・字幕・口パク） | `話者: セリフ`。字幕・口パクは自動 |
| 立ち絵アイテム / 表情アイテム | `- id: キャラ@バリエーション {…}`、セリフの `face:` / `話者@表情:` |
| 口パク（あいうえお）・目パチ | キャラクター定義の `base` / `mouth` / `eyes`（音声を解析して母音を判定） |
| テキストアイテム | `- text:`（縁取り・影・背景・太字・斜体・縦書き・文字送り） |
| 画像アイテム | `- image:`（`overlay:` も同じ） |
| 動画アイテム | `- video:`（H.264 の MP4。純 Rust でデコード） |
| 図形アイテム | `- shape:`（四角形・円・三角形・星・線） |
| 音声アイテム（BGM・効果音） | `- bgm:` / `- sound:` / セリフの `se:` |
| レイヤー | `layer:`（大きいほど手前） |
| 開始位置・長さ | `at:` / `until:` / `dur:`（`L2` で 2 番目のセリフに合わせる） |
| X・Y・拡大率・回転・不透明度・左右反転 | `x:` `y:` `scale:` `rotate:` `opacity:` `flip` |
| 中間点・移動方法 | `0 400@1.5~ease-out 300@end` |
| 合成モード | `blend:` |
| 表示/非表示アニメーション | `in:` / `out:` |
| 映像エフェクト | `fx:` |
| グループ制御（全体を動かす） | `- camera:` |
| 場面切り替え | `- transition:` |
| 読み（発音）・速度・音量・高さ・間 | ルビ `｜漢字《よみ》` / `read:`、`speed:` `volume:` `pitch:` `pre:` `wait:` |

YMM4 にあって MarkdownMovieMaker にないもの（2026 年 11 月時点）: タイムライン上のマウス操作、PSD 立ち絵の直接読み込み、
音声エフェクト（エコーなど）、字幕のアニメーションのテンプレート。

## シーンの設定 {#scene}

| 行 | 説明 |
| --- | --- |
| `- bg: パス {…}` | 背景。省略すると前のシーンの背景（と属性）を引き継ぐ。属性で拡大・移動・エフェクトを掛けられる |
| `- before_pause: 秒` / `- after_pause: 秒` | シーンの前後の間 |
| `- length: 秒` | シーンの最短の長さ（セリフのないタイトル画面など）。足りない分は終わりの間を延ばす |
| `- speed: 倍率` | このシーンのセリフの読み上げ速度 |
| `- id: キャラ@バリエーション.エフェクト {…}` | 立ち絵（下記） |
| `- text:` / `- image:` / `- overlay:` / `- video:` / `- shape:` | アイテム（下記） |
| `- bgm: パス {…}` / `- sound: パス [長さ] [開始秒] {…}` | 音声（下記） |
| `- transition: 種類 [秒] [補間]` | 前のシーンからの場面切り替え |
| `- camera: {…}` | 字幕より下の全体に掛ける拡大・移動・回転・エフェクト |

### 場面切り替え {#transition}

`fade`（クロスフェード）、`black`（暗転）、`white`、`wipe-left` / `wipe-right` / `wipe-up` / `wipe-down`、
`slide-left` / `slide-right` / `slide-up` / `slide-down`、`circle`（円）、`zoom`、`mosaic`、
`shatter`（前のシーンがガラスのように割れて飛び散り、新しいシーンが現れる）。

```markdown
- transition: circle 0.8 ease-in-out
```

## アイテム共通の属性 {#props}

| 属性 | 説明 | 例 |
| --- | --- | --- |
| `x`, `y` | 中心の位置。画面中央が原点（基準解像度 1920×1080 のピクセル） | `x: -500`, `y: -380` |
| `scale` (`zoom`), `scale_x`, `scale_y` | 拡大率（`1.2` または `120%`） | `scale: 80%` |
| `rotate` | 回転（度、時計回り） | `rotate: -4` |
| `opacity` (`alpha`) | 不透明度 | `opacity: 0.8` |
| `flip`, `flipv` | 左右・上下反転 | `flip` |
| `blend` | 合成モード: `normal` `add` `multiply` `screen` `overlay` `lighten` `darken` | `blend: screen` |
| `layer` | 重なり順（大きいほど手前）。既定: 動画・図形 5、立ち絵 10、画像 50、テキスト 60。100 以上は字幕より手前 | `layer: 40` |
| `at`, `until`, `dur` | 表示の開始・終了・長さ（下記の時刻） | `at: L2, until: L3.end` |
| `in`, `out` | 表示・非表示のアニメーション `種類 [秒] [補間]` | `in: slide-left 0.4` |
| `fx` | 映像エフェクト（空白区切りで複数） | `fx: shadow(8,8) glow(10)` |

### 時刻（`at` / `until`） {#time}

| 書き方 | 意味 |
| --- | --- |
| `1.5` | シーンの開始から 1.5 秒 |
| `L2` / `L2+0.3` | 2 番目のセリフ（の声）が始まる時刻 / その 0.3 秒後 |
| `L2.end` | 2 番目のセリフが終わる時刻 |
| `end-1` | シーンの終わりの 1 秒前 |

省略時はシーン全体に表示します。

### アニメーション（中間点） {#keyframes}

数値の属性は時間で変えられます。時刻はアイテムの開始からの秒、`%`（長さに対する割合）、`end`、`end-秒`。

```text
x: -800->800                 # 開始から終了まで直線で動く
x: -800->800 ease-out        # 減速しながら
y: 0 -40@0.5~bounce 0@1.2    # 中間点ごとに時刻と補間（~ の後）を指定
rotate: 0 10 0               # 時刻を省くと等間隔
```

補間: `linear`（直線）、`ease-in`（加速）、`ease-out`（減速）、`ease-in-out`、`back`、`bounce`、`elastic`、`step`。

### 表示・非表示アニメーション（`in` / `out`） {#motion}

`fade`、`zoom`、`pop`、`spin`、`blur`、`drop`、`slide-left` / `slide-right` / `slide-up` / `slide-down`、
`wipe-left` / `wipe-right` / `wipe-up` / `wipe-down`、`shatter`（割れる）。既定は 0.3 秒・`ease-out`。

#### ぱりーんと割れる（`shatter`） {#shatter}

```markdown
- id: zundamon@default_left {out: shatter 1.0}              # シーンの終わりに割れて飛び散る
- image: data/overlay/a.png {until: 3.2, out: shatter 0.9}  # 3.2 秒で割れる
- text: 復活！ {in: shatter 0.8}                             # 破片が集まって現れる
- transition: shatter 1.2                                    # 前のシーンが割れて次のシーンへ
```

まずひびが入り（破片の間に隙間が開く）、次に中心から外へ飛び散りながら回転し、重力で落ちて消えます。
破片の形と動きはアイテムごとに決まっているので、プレビューと書き出しで同じ絵になります。
効果音は `se:` や `- sound:` で合わせてください。

### 映像エフェクト（`fx`） {#fx}

| 名前 | 引数（既定値） | 内容 |
| --- | --- | --- |
| `blur` (`ぼかし`) | 半径 (6) | ぼかし |
| `shadow` (`影`) | dx, dy, ぼかし, 色 (6, 6, 8, #000000a0) | ドロップシャドウ |
| `glow` (`発光`) | 半径, 色 (12, white) | 外側の光 |
| `outline` (`縁取り`) | 太さ, 色 (4, white) | 縁取り |
| `mosaic` (`モザイク`) | 大きさ (12) | モザイク |
| `grayscale` `sepia` `negative` | — | モノクロ・セピア・ネガ |
| `brightness` `contrast` `saturation` | 倍率 | 明るさ・コントラスト・彩度 |
| `hue` | 角度 (30) | 色相 |
| `tint` | 色 | 色を掛ける |
| `chroma` (`クロマキー`) | 色, 許容差 (#00ff00, 0.3) | 指定色を透過 |
| `shake` (`振動`) | 幅, 速さ (8, 30) | ランダムに揺れる |
| `bounce` (`跳ねる`) | 高さ, 速さ (20, 5) | 跳ねる |
| `float` (`浮遊`) | 幅, 周期 (10, 2) | 上下にゆっくり |
| `sway` (`揺れ`) | 角度, 周期 (5, 1.5) | 左右に傾く |
| `spin` (`回転`) | 毎秒の角度 (90) | 回り続ける |
| `pulse` (`鼓動`) | 量, 周期 (0.08, 0.8) | 拡大縮小を繰り返す |
| `blink` (`点滅`) | 周期 (0.5) | 点滅 |
| `afterimage` (`残像`) | 数, 間隔 (4, 0.05) | 残像 |

## アイテム {#items}

### テキスト `- text:` {#text}

```markdown
- text: "1 行目\n2 行目" {size: 80, color: #ffffff, outline: 6 #000 | 12 #fff, shadow: 4 4 6 #00000099, box: #000000a0 20 12, bold, align: left, typewriter: 0.05}
```

| 属性 | 説明 |
| --- | --- |
| `size`, `color`, `font` | 文字の大きさ（ピクセル）・色・フォントファイル |
| `outline` | 縁取り `太さ 色`。`|` 区切りで多重（内側から） |
| `shadow` | 影 `dx dy ぼかし 色` |
| `box` | 背景の箱 `塗り色 余白 角の半径 [枠の太さ 枠の色]` |
| `bold`, `italic`, `vertical` | 太字・斜体・縦書き |
| `align` | `left` / `center` / `right` |
| `spacing`, `line_height` | 文字間隔（ピクセル）・行の高さ（倍率、既定 1.2） |
| `width` | 折り返す幅（ピクセル） |
| `typewriter` | 1 文字ずつ表示する間隔（秒） |

### 画像 `- image:` / `- overlay:` {#image}

元の大きさで画面中央に置きます（Python 版の `overlay` と同じ）。アニメーション GIF も使えます。

SVG も使えます（`- bg:` の背景にも）。`width` / `height`（なければ `viewBox`）の大きさで描きます。
文字には同梱の M+ 1 Bold、OS のフォント、素材フォルダーの `data/fonts` を使います。
`font-family` で数字を含む名前は `'M PLUS 1 Code'` のように引用符で囲んでください。

### 動画 `- video:` {#video}

```markdown
- video: data/movie/clip.mp4 {scale: 50%, x: 400, volume: 0.5, trim: 2, loop}
```

`volume`（音量）、`speed`（再生速度。1 以外では音声を使いません）、`trim`（開始位置の秒）、`loop`、`mute`。
H.264 の MP4 に対応します（ffmpeg は使いません）。

### 図形 `- shape:` {#shape}

```markdown
- shape: rect {w: 600, h: 120, color: #00000080, radius: 20, stroke: 4 white}
```

`rect`（四角形）、`ellipse`（円）、`triangle`、`star`、`line`。`w` `h` `color` `stroke` `radius` `nofill`。

## 立ち絵 `- id:` {#character}

```markdown
- id: zundamon@default_left.bouncing {x: -500, flip, in: slide-left 0.5, fx: outline(6, white)}
```

- `@バリエーション` を省くと `default`。`.エフェクト` は Python 版の `bouncing`（話している間だけ跳ねる）/ `shaking` / `grayscale` / `shatter`（シーンの終わりに割れる。`out: shatter 0.8` と同じ）。
- 位置の既定はキャラクター定義の `pos`（左上）。`x` / `y` を書くと中心をそこへ動かします。
- 話しているキャラクターは同じレイヤーの中で一番手前に描きます。

## セリフ {#lines}

```markdown
## Lines
zundamon: ｜詐欺《さぎ》の話なのだ！ {face: smile, speed: 1.1, pitch: 1.1, volume: 1.2, pre: 0.2, wait: 0.5, motion: bounce(30), se: data/sound/pafu1.mp3}
metan@komatta: 困りましたわ。
metan@noscript: 字幕なしのセリフ
```

| 書き方 | 説明 |
| --- | --- |
| `話者@noscript:` / `{subtitle: off}` | 字幕を出さない |
| `話者@表情:` / `{face: 表情}` | このセリフの間だけ立ち絵のバリエーションを変える |
| `｜表示《読み》`、`漢字《かんじ》` | ルビ。字幕は表示側、読み上げは読み側 |
| `{read: よみ}` | 読み上げる文を丸ごと指定 |
| `speed`, `pitch`, `volume` | 速度・声の高さ・音量の倍率 |
| `pre`, `wait` | セリフの前・後の間（秒） |
| `motion` | 話している間だけ話者に掛けるエフェクト（`bounce(30)`、`shake(4)` など） |
| `se` | セリフの始まりに鳴らす効果音 |

## BGM・効果音 {#audio}

```markdown
- bgm: data/bgm/001.mp3 {volume: 0.2, fade_in: 1, fade_out: 2, at: L2, until: end, continue}
- sound: data/sound/pafu1.mp3 0.5 1.2 {volume: 0.8}
- sound: data/sound/kira1.mp3 {at: L3+0.2}
```

- `bgm` はシーンの間ループし、終わりでフェードアウトします（既定 1 秒）。`continue` で次に `bgm` を持つシーンまで流し続けます。
  シーンの BGM がある間は全体 BGM（`--bgm`）を止めます。
- `sound` の 2 番目は最大の長さ、3 番目はシーン開始からの位置（秒）。`at` で `L2` などにも合わせられます。

## キャラクター定義（`data/character_master.json`） {#master}

Python 版の項目（`name` `speaker_id` `variations` `subtitle_style`）に加えて:

```json
"zundamon": {
    "speaker_id": 3,
    "voice": {"engine": "voicevox", "language": "ja", "speed": 1.0, "pitch": 1.0},
    "blink_interval": 4.0,
    "variations": {
        "parts": {
            "base": "data/character/z_body.png",
            "eyes": ["data/character/z_eye_open.png", "data/character/z_eye_half.png", "data/character/z_eye_closed.png"],
            "mouth": {"a": "data/character/z_a.png", "i": "data/character/z_i.png", "u": "data/character/z_u.png",
                      "e": "data/character/z_e.png", "o": "data/character/z_o.png", "n": "data/character/z_n.png"},
            "scale": 0.9, "pos": [1200, 300]
        }
    },
    "subtitle_style": {"fontsize": 70, "color": "green", "border_color": "green",
                       "outline": "4 #ffffff", "shadow": "3 3 4 #00000080", "typewriter": 0.03, "bottom": 0.95}
}
```

- **パーツ方式の立ち絵**: `base`（目・口を除いた体）に、目（`eyes`: 開→閉の順）と口（`mouth`: あいうえお＋`n` 閉じ）を重ねます。
  すべて同じ大きさの透過 PNG にしてください。口は音声のフォルマントから母音を判定して切り替え、足りない母音は近い形で代用します。
  目は `blink_interval` 秒（平均）ごとにまばたきします。
- 従来の `image_closed` / `image_open` の 2 枚方式もそのまま使えます（声が出ている間 10Hz で開閉）。
- `subtitle_style` には `font`、`outline`、`shadow`、`no_box`（背景の箱を消す）、`typewriter`、`bottom`（字幕の下端の位置）を追加できます。

## コマンドライン（mmm） {#cli}

同じ処理をコマンドラインの `mmm` でも行えます（Windows・Linux・macOS）。

```text
mmm scripts/sample.md --out data/outputs/sample.mp4
mmm scripts/sample.md --audio-only --out sample.mp3
mmm scripts/sample.md --image-only --out stills/sample.png
mmm voices --engine voicevox
```

主なオプション: `--engine`、`--device`、`--size`、`--fps`、`--encoder`、`--fast-encode`、`--bgm`、`--chars`、`--root`。
`mmm --help` で一覧を表示します。

## キーボードショートカット {#shortcuts}

| キー | 動作 |
| --- | --- |
| F1 | ヘルプ（場所に応じた項目を開く） |
| F5 | このシーンを再生・停止 |
| Shift+F5 | 通して再生・停止 |
| F6 | 音声を合成 |
| F7 | キャレットのあるセリフを合成して再生 |
| Ctrl+PageUp / Ctrl+PageDown | 前・次のシーン |
| Ctrl+N / Ctrl+O / Ctrl+S | 新規・開く・保存 |
| Ctrl+E | 書き出し |
| ↑ / ↓、Enter（スライドで） | 前後のシーン、このシーンを再生 |

## 困ったときは {#trouble}

- **画像が表示されない**: [メッセージ](#messages)の「素材: …」を確認してください。台本のパスは素材の基準フォルダー（ステータスバー）からの相対パスです。
- **声が出ない・合成に失敗する**: 初回はモデルのダウンロードが必要です。VOICEVOX はエンジンを起動してから合成してください。
- **再生しても音が出ない**: 音声出力デバイスを確認してください。出力がない環境では映像だけを再生します。
- **キャラクター定義を直したのに反映されない**: 「プロジェクト → 素材・キャラクター定義を読み直す」を選んでください。
- **ヘルプが文字だけで表示される**: WebView2 ランタイムがない環境では、ヘルプを文字だけで表示します。
