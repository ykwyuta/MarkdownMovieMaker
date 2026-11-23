<!--
  yyeditor の紹介（解説: ずんだもん / 質問: 春日部つむぎ）
  音声は VOICEVOX を使います。VOICEVOX を起動してから次のどちらかで合成してください。
    CLI: mmm scripts/zundamon_tumugi_yyeditor_intro.md --engine voicevox --out data/outputs/yyeditor_intro.mp4
    GUI: 上部の音声エンジンで「VOICEVOX」を選んでから「音声を合成」(F6)
  背景は data/yyeditor_intro/bg/*.svg。実際の yyeditor の画面キャプチャ（data/yyeditor_intro/cap/）に注釈を付けたもの。
  内容の出典: references/yyeditor の README.ja.md、docs/user/applications.ja.md、crates/yy-win/help/help.md
-->

# Scene@タイトル

- bg: data/yyeditor_intro/bg/00_title.svg {scale: 1.08->1}
- length: 4
- bgm: data/bgm/002.mp3 {volume: 0.12, fade_in: 1, continue}
- sound: data/sound/presentation-title1.mp3 {at: 0.2}

# Scene@導入

- bg: data/bg/PC_Room_Daytime.jpg
- transition: fade 0.6
- id: zundamon@default_left {in: slide-left 0.5}
- id: tumugi@default {in: slide-right 0.5}

## Lines
tumugi: ずんだもん先輩、この前の動画に出てきた｜yyeditor《ワイワイエディター》って、どんなエディタなんですか？
tumugi: 名前を聞いたことがなくて、ちょっと気になってたんです。
zundamon: ふっふっふ、知る人ぞ知るエディタなのだ。今日は実際の画面を見ながら紹介するのだ！ {face: smile_left, se: data/sound/kira1.mp3}
tumugi: お願いします！ {face: smile}

# Scene@プロフィール

- bg: data/yyeditor_intro/bg/01_profile.svg
- transition: wipe-left 0.5
- id: zundamon@default_left {x: -790, y: 330, scale: 0.72}
- id: tumugi@default {x: 790, y: 330, scale: 0.72}

## Lines
zundamon: ｜yyeditor《ワイワイエディター》は、数ギガバイトのファイルもすぐ開ける、ウィンドウズ向けの軽いテキストエディタなのだ。
zundamon: ラストで書かれていて、画面はウィンドウズの部品で描いているのだ。ライセンスはジーピーエルなのだ。
tumugi: 下の青いのが、得意なことですか？
zundamon: そうなのだ。巨大ファイル、日本語の文字コード、｜EBCDIC《エビシディック》、 {motion: bounce(30)}
zundamon: ｜CSV《シーエスブイ》、バイナリ、マークダウン、ギット……盛りだくさんなのだ。
tumugi: 欲張りセットですね！ {face: smile}

# Scene@巨大ファイル

- bg: data/yyeditor_intro/bg/02_bigfile.svg
- transition: slide-left 0.5
- id: zundamon@default_left {x: -790, y: 330, scale: 0.72}
- id: tumugi@default {x: 790, y: 330, scale: 0.72}

## Lines
zundamon: まずは一番の得意技、巨大ファイルなのだ。
zundamon: これは約1ギガバイト、954メガバイトのログを、500万行目を指定して開いたところなのだ。
tumugi: えっ、もう500万行目が表示されてる！ {face: yes, se: data/sound/surprise1.mp3}
zundamon: ファイルを全部読み込まずに、メモリマップで必要なところだけ読むから、開くのは一瞬なのだ。
zundamon: 行数は裏で数えるから、数え終わるまでの行番号は、灰色の推定値で表示するのだ。
tumugi: ログのINFOやERRORにも、ちゃんと色が付いてますね。

# Scene@ソースコード

- bg: data/yyeditor_intro/bg/03_code.svg
- transition: slide-left 0.5
- id: zundamon@default_left {x: -790, y: 330, scale: 0.72}
- id: tumugi@default {x: 790, y: 330, scale: 0.72}

## Lines
zundamon: もちろん、プログラムも書けるのだ。これは｜yyeditor《ワイワイエディター》自身のラストのソースなのだ。
zundamon: ラスト、シー、パイソン、ジャバスクリプト、コボル、｜JCL《ジェイシーエル》など、38種類の言語を色分けできるのだ。
tumugi: 行の頭の点々と、矢印みたいなのは何ですか？
zundamon: 空白と改行の印なのだ。全角スペースが混ざってもすぐ気づけるのだ。
zundamon: ファイルの種類は自動で判別して、右下に表示されるのだ。

# Scene@マルチカーソル

- bg: data/yyeditor_intro/bg/04_multi.svg
- transition: circle 0.6
- id: zundamon@default_left {x: -790, y: 330, scale: 0.72}
- id: tumugi@default {x: 790, y: 330, scale: 0.72}

## Lines
zundamon: 編集の便利機能も見てほしいのだ。左は「マルチカーソル」なのだ。
zundamon: 「Ctrl」を検索して一致箇所をすべて選ぶと、
zundamon: 3か所にカーソルが置かれて、同時に「Alt」と書き換えられたのだ。
tumugi: 3か所まとめて直せるんですね！ {face: yes}
zundamon: 右は「矩形選択」なのだ。
zundamon: アルトとシフトを押しながら矢印キーで、3行かける6桁の四角い範囲を選んだのだ。
zundamon: 同梱のフォントは、全角がちょうど半角2つ分の幅だから、日本語が混ざっても四角がずれないのだ。

# Scene@検索

- bg: data/yyeditor_intro/bg/05_search.svg
- transition: wipe-left 0.5
- id: zundamon@default_left {x: -790, y: 330, scale: 0.72}
- id: tumugi@default {x: 790, y: 330, scale: 0.72}

## Lines
tumugi: 検索はどうですか？
zundamon: コントロールエフで検索バーが開いて、入力するたびに一致した場所へ移動して、全部に色が付くのだ。
zundamon: 大文字と小文字の区別、単語単位、正規表現も選べるのだ。置換もできるのだ。
tumugi: 下のボタンの「フォルダ内を検索」も気になります。

# Scene@Grep

- bg: data/yyeditor_intro/bg/06_grep.svg
- transition: slide-left 0.5
- id: zundamon@default_left {x: -790, y: 330, scale: 0.72}
- id: tumugi@default {x: 790, y: 330, scale: 0.72}

## Lines
zundamon: それが｜Grep《グレップ》なのだ。フォルダの中のファイルを、まとめて検索するのだ。
zundamon: ここでは537個のファイルから、9件見つかったのだ。結果は新しいタブに一覧で出るのだ。
zundamon: 結果の行で｜F12《エフじゅうに》を押すと、そのファイルのその行へ一発でジャンプするのだ。
tumugi: 探してから開くまでが、すごく速いです！ {face: smile}

# Scene@文字コード

- bg: data/yyeditor_intro/bg/07_encoding.svg
- transition: wipe-up 0.5
- id: zundamon@default_left {x: -790, y: 330, scale: 0.72}
- id: tumugi@default {x: 790, y: 330, scale: 0.72}

## Lines
tumugi: 古いシステムの、シフトジスのファイルも開けますか？
zundamon: 任せるのだ。これはシフトジスで保存したファイルを、そのまま開いたところなのだ。
zundamon: 文字コードは自動で判別して、右下に｜CP932《シーピーきゅうさんに》、改行は｜CRLF《シーアールエルエフ》と表示しているのだ。
zundamon: メインフレームで使う｜EBCDIC《エビシディック》まで扱えるのは、かなり珍しいのだ。
zundamon: 壊れたバイトがあっても、同じ文字コードで保存すれば元どおりのバイトに戻るのだ。
tumugi: 開いて保存しただけで壊れない、って安心ですね。

# Scene@CSV

- bg: data/yyeditor_intro/bg/08_csv.svg
- transition: slide-left 0.5
- id: zundamon@default_left {x: -790, y: 330, scale: 0.72}
- id: tumugi@default {x: 790, y: 330, scale: 0.72}

## Lines
zundamon: ｜CSV《シーエスブイ》を開くと、列をそろえて表のように表示するのだ。
zundamon: 上には、エクセルと同じA、B、Cの列名が出るのだ。
tumugi: 1004番の人のメモは、改行が入ってるのに1つのセルになってますね。
zundamon: 引用符の中の改行も、ちゃんと理解しているのだ。
zundamon: タブキーでセルを移動したり、列を足したり消したりもできるのだ。

# Scene@バイナリ

- bg: data/yyeditor_intro/bg/09_binary.svg
- transition: slide-left 0.5
- id: zundamon@default_left {x: -790, y: 330, scale: 0.72}
- id: tumugi@default {x: 790, y: 330, scale: 0.72}

## Lines
zundamon: テキストだけじゃないのだ。左は16進数表示で、ファイルのバイトを直接見たり書き換えたりできるのだ。
zundamon: 右は「固定長表示」なのだ。
zundamon: 改行のない、40バイトずつのレコードのファイルを、1件ずつ並べているのだ。
tumugi: 上の目盛りと、16進数と、文字の3段で、桁がそろってますね！ {face: yes}
zundamon: 業務システムのデータを調べるときに、とっても役に立つのだ。

# Scene@ちょっと休憩

- bg: data/bg/ConcentrationLines1_Blue.jpg
- transition: zoom 0.4
- id: zundamon@smile_left_closeup {fx: shake(6, 20)}
- text: 機能、多すぎなのだ！ {y: -380, size: 80, color: #ffffff, outline: 6 #1d4ed8, at: L1, in: pop 0.4, layer: 60}

## Lines
zundamon: ここまでで、まだ半分なのだ！ {motion: bounce(40), se: data/sound/boyon1.mp3}
tumugi: 先輩、息つぎしてください。 {se: data/sound/tsukkomi-1.mp3}

# Scene@Markdown

- bg: data/yyeditor_intro/bg/10_markdown.svg
- transition: zoom 0.4
- id: zundamon@default_left {x: -790, y: 330, scale: 0.72}
- id: tumugi@default {x: 790, y: 330, scale: 0.72}

## Lines
zundamon: 後半は書く機能なのだ。
zundamon: マークダウンを書いて、コントロール、シフト、ブイを押すと、右側にプレビューが出るのだ。
zundamon: 表も、｜Mermaid《マーメイド》の図も、数式も、そのまま描いてくれるのだ。
tumugi: 左で書いたら、右にすぐ反映されるんですか？
zundamon: 0.25秒くらいで反映されて、スクロールも連動するのだ。

# Scene@ライブ編集

- bg: data/yyeditor_intro/bg/11_liveedit.svg
- transition: slide-left 0.5
- id: zundamon@default_left {x: -790, y: 330, scale: 0.72}
- id: tumugi@default {x: 790, y: 330, scale: 0.72}

## Lines
zundamon: さらに「マークダウン、ライブ編集」もあるのだ。表示されたまま、その場で書けるモードなのだ。
zundamon: 上のボタンで太字や見出しを付けたり、表の行や列を足したりできるのだ。
tumugi: 記号を覚えてなくても書けるのは、うれしいです！ {face: smile}
zundamon: クリックした段落だけ原文に戻るから、細かい直しも簡単なのだ。

# Scene@ワークスペース

- bg: data/yyeditor_intro/bg/12_workspace.svg
- transition: wipe-left 0.5
- id: zundamon@default_left {x: -790, y: 330, scale: 0.72}
- id: tumugi@default {x: 790, y: 330, scale: 0.72}

## Lines
zundamon: 最後は、｜VS Code《ブイエスコード》みたいな使い方なのだ。
zundamon: 左のサイドバーに、いくつものフォルダをツリーで並べられるのだ。
zundamon: ソース管理に切り替えると、ギットのブランチや変更が見えて、コミットやプッシュもできるのだ。
tumugi: 下に書いてある｜SSH《エスエスエイチ》というのは？
zundamon: 遠くのリナックスのサーバーのファイルも、手元のファイルと同じように開いて保存できるのだ。

# Scene@仲間たち

- bg: data/yyeditor_intro/bg/13_family.svg
- transition: fade 0.6
- id: zundamon@default_left {x: -790, y: 330, scale: 0.72}
- id: tumugi@default {x: 790, y: 330, scale: 0.72}

## Lines
zundamon: 実は｜yyeditor《ワイワイエディター》には、同じ部品で作られた仲間のアプリがたくさんあるのだ。
zundamon: ターミナル、ファイル転送、表計算、比較ツール、
zundamon: データベース、作図……と、ワイワイシリーズになっているのだ。
tumugi: エディタだけじゃなかったんですね！ {face: yes}

# Scene@まとめ

- bg: data/yyeditor_intro/bg/14_summary.svg
- transition: fade 0.8
- id: zundamon@smile_left {x: -790, y: 330, scale: 0.72}
- id: tumugi@smile {x: 790, y: 330, scale: 0.72}

## Lines
zundamon: まとめるのだ！ 巨大ファイルを一瞬で開けて、日本語や｜EBCDIC《エビシディック》の文字コードに強いのだ。
zundamon: ｜CSV《シーエスブイ》、固定長、バイナリといった業務データも、そのまま見て直せるのだ。
zundamon: そして、マークダウン、ギット、｜SSH《エスエスエイチ》で、書く、管理する、つなぐまで、これ1つなのだ！ {motion: bounce(30), se: data/sound/kira2.mp3}
tumugi: 名前は知らなくても、実力はすごいエディタでした！

# Scene@エンディング

- bg: data/bg/PC_Room_Daytime.jpg
- transition: fade 0.6
- length: 3
- id: zundamon@smile_left
- id: tumugi@smile
- text: ご視聴ありがとうございました {y: -300, size: 80, color: #ffe000, outline: 6 #000000 | 12 #ffffff, at: L1, in: pop 0.5, layer: 60}

## Lines
zundamon: 最後まで見てくれてありがとうなのだ。気になったら、ぜひ使ってみてほしいのだ！ {wait: 0.5}
tumugi: ありがとうございました！
