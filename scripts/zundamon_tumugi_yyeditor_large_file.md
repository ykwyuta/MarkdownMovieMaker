<!--
  yyeditor が巨大ファイルを扱うしくみの解説（解説: ずんだもん / 質問: 春日部つむぎ）
  音声は VOICEVOX を使います。VOICEVOX を起動してから次のどちらかで合成してください。
    CLI: mmm scripts/zundamon_tumugi_yyeditor_large_file.md --engine voicevox --out data/outputs/yyeditor_large_file.mp4
    GUI: 上部の音声エンジンで「VOICEVOX」を選んでから「音声を合成」(F6)
  話者 ID は data/character_master.json の speaker_id（ずんだもん 3、春日部つむぎ 8）を使います。
  図解の背景は data/yyeditor_large_file/bg/*.svg（ソースの抜粋入り）。
  内容の出典: references/yyeditor の crates/yy-buffer, yy-io, yy-core, yy-layout と
  docs/history/proposals/02-buffer-large-file.md
-->

# Scene@タイトル

- bg: data/bg/PC_Room_Daytime.jpg {scale: 1.15->1, fx: blur(6)}
- length: 4
- shape: rect {w: 1920, h: 300, color: #000000b0, y: -40, in: wipe-left 0.6, layer: 30}
- text: "10GBのファイルを一瞬で開く！" {y: -90, size: 100, color: #ffe000, outline: 6 #000000 | 12 #ffffff, shadow: 6 6 8 #00000099, in: pop 0.6, layer: 40}
- text: yyeditor 巨大ファイルのしくみ {y: 40, size: 56, color: white, at: 0.8, in: fade 0.5, typewriter: 0.05, layer: 40}
- bgm: data/bgm/001.mp3 {volume: 0.12, fade_in: 1, continue}
- sound: data/sound/presentation-title1.mp3 {at: 0.2}

# Scene@導入

- bg: data/bg/PC_Room_Daytime.jpg
- transition: fade 0.6
- id: zundamon@default_left {in: slide-left 0.5}
- id: tumugi@default {in: slide-right 0.5}

## Lines
tumugi: ずんだもん先輩、聞いてください！ {motion: bounce(20)}
tumugi: 会社のサーバーから10｜GB《ギガバイト》のログファイルをもらったんですけど、メモ帳で開いたら固まっちゃいました。 {face: komatta}
zundamon: ふっふっふ、それは「全部読み込んでから表示する」エディタの宿命なのだ。
tumugi: 全部読み込むのって、ダメなんですか？

# Scene@全部読む

- bg: data/yyeditor_large_file/bg/01_naive.svg
- transition: wipe-left 0.5
- id: zundamon@default_left {x: -790, y: 330, scale: 0.72}
- id: tumugi@default {x: 790, y: 330, scale: 0.72}

## Lines
zundamon: 10｜GB《ギガバイト》を全部メモリに載せたら、読むだけで何十秒もかかるし、メモリも10｜GB《ギガバイト》以上使っちゃうのだ。
tumugi: うわぁ、あーしのパソコンだと、それだけでいっぱいになっちゃいます。 {face: komatta}
zundamon: そこで今日は、｜yyeditor《ワイワイエディター》というエディタが、 {face: smile_left, se: data/sound/kira1.mp3}
zundamon: 巨大ファイルをどうやって一瞬で開いているのかを、ソースコードを見ながら解説するのだ！ {face: smile_left}
tumugi: よろしくお願いします、先輩！ {face: smile}

# Scene@全体像

- bg: data/yyeditor_large_file/bg/02_overview.svg
- transition: fade 0.5
- id: zundamon@default_left {x: -790, y: 330, scale: 0.72}
- id: tumugi@default {x: 790, y: 330, scale: 0.72}

## Lines
zundamon: 先に結論から言うのだ。ポイントは四つあるのだ。
zundamon: ひとつめ、ファイルを読み込まずに「見るだけ」にする。ふたつめ、元のファイルは書き換えない。
zundamon: みっつめ、木の形でまとめて、欲しい場所をすぐ探す。
zundamon: よっつめ、時間のかかる仕事は裏でこっそりやるのだ。
tumugi: 四つとも、なんだか「サボり方」の話に聞こえますね。
zundamon: 鋭いのだ！ 巨大ファイルのコツは、まさに「やらなくていい仕事はやらない」ことなのだ。 {face: smile_left, motion: bounce(30)}

# Scene@1. メモリマップ

- bg: data/yyeditor_large_file/bg/03_mmap.svg
- transition: slide-left 0.5
- id: zundamon@default_left {x: -790, y: 330, scale: 0.72}
- id: tumugi@default {x: 790, y: 330, scale: 0.72}

## Lines
zundamon: ひとつめは「メモリマップ」なのだ。英語だと｜mmap《エムマップ》っていうのだ。
zundamon: ファイルの中身をコピーするんじゃなくて、「このメモリの番地を見たら、
zundamon: ファイルのこの場所が見える」っていう対応づけだけをOSにお願いするのだ。
tumugi: 対応づけるだけ、ですか？
zundamon: そうなのだ。実際にデータを読むのは、画面に表示するために触ったところだけなのだ。
zundamon: ソースだと、黄色い行の｜Mmap::map《エムマップのマップ》だけで、10｜GB《ギガバイト》のファイルでもすぐ表示できる準備が終わるのだ。
tumugi: 図書館で本を全部借りずに、読みたいページだけ開く感じですね！ {face: yes}
zundamon: その例え、いただきなのだ。 {face: smile_left}

# Scene@1. 鍵

- bg: data/yyeditor_large_file/bg/04_lock.svg
- id: zundamon@default_left {x: -790, y: 330, scale: 0.72}
- id: tumugi@default {x: 790, y: 330, scale: 0.72}

## Lines
tumugi: でも先輩、見てる途中で、ほかのアプリがファイルを書き換えたらどうなるんですか？ {face: komatta}
zundamon: いい質問なのだ。メモリマップ中に中身が変わると、とっても危ないのだ。
zundamon: だから｜yyeditor《ワイワイエディター》は、ファイルを開くとき「読むのと削除はいいけど、
zundamon: 書き込みはダメ」という鍵をかけて開いているのだ。
zundamon: ソースの｜share_mode《シェアモード》で、読み取りと削除の共有だけを許しているのが、その鍵なのだ。
tumugi: 削除はいいんですか？
zundamon: それは保存のときに使うワザなのだ。あとで説明するから覚えておいてほしいのだ。

# Scene@2. ピース

- bg: data/yyeditor_large_file/bg/05_piece.svg
- transition: slide-left 0.5
- id: zundamon@default_left {x: -790, y: 330, scale: 0.72}
- id: tumugi@default {x: 790, y: 330, scale: 0.72}

## Lines
zundamon: ふたつめは「ピース」という考え方なのだ。
tumugi: ピース？ 平和ですか？ {face: smile, se: data/sound/question1.mp3}
zundamon: 平和もずんだ餅も大事だけど、ここでは「かけら」のことなのだ。 {face: smile_left}
zundamon: 文書を「どのソースの、何バイト目から、何バイト」という参照の並びで表すのだ。
zundamon: たとえば「ずんだ餅おいしい」の真ん中に「とても」と打ち込むと、
zundamon: 「とても」は新しい小さなソースに入れて、ピースを一つ差し込むだけなのだ。
tumugi: じゃあ、元のファイルは一文字も書き換えないんですね。 {face: yes}
zundamon: そうなのだ。ソースのほうも、一度作ったら二度と書き換えない決まりなのだ。
zundamon: 変わらないから、昔の版のピースがずっと同じ場所を指していても安全なのだ。

# Scene@3. 分割

- bg: data/yyeditor_large_file/bg/06_split.svg
- id: zundamon@default_left {x: -790, y: 330, scale: 0.72}
- id: tumugi@default {x: 790, y: 330, scale: 0.72}

## Lines
zundamon: しかも元ファイルは、開いた瞬間に1｜MiB《メビバイト》ずつのピースに分けておくのだ。
zundamon: 10｜GB《ギガバイト》なら、だいたい1万個のピースになるのだ。この時点では改行の数はまだ数えないのだ。
zundamon: それから、日本語の文字は3バイトだから、
zundamon: 切れ目が文字の途中に来たら、最大3バイト戻って文字の境目で切るのだ。
tumugi: 1万個も並んでたら、探すのが大変じゃないですか？ {face: komatta}
zundamon: そこで、みっつめの「木」の出番なのだ！ {motion: bounce(30), se: data/sound/shakin1.mp3}

# Scene@3. 永続B+木

- bg: data/yyeditor_large_file/bg/07_tree.svg
- transition: circle 0.6
- id: zundamon@default_left {x: -790, y: 330, scale: 0.72}
- id: tumugi@default {x: 790, y: 330, scale: 0.72}

## Lines
zundamon: ピースは「｜B+木《ビープラスぎ》」という木の形に並べてあるのだ。
zundamon: 木の各ノードは、自分より下にあるピースの合計、
zundamon: つまりバイト数や改行の数を｜Summary《サマリー》として覚えているのだ。
tumugi: 合計を覚えておくと、何がいいんですか？
zundamon: たとえば「100万行目に行きたい」と言われたら、
zundamon: 上から改行の数を見て、どっちの枝に進めばいいか一発でわかるのだ。
zundamon: 枝分かれは最大32本だから、何万個のピースがあっても、
zundamon: ほんの数段たどるだけで目的のピースに着くのだ。
tumugi: 社員名簿で、部署ごとの人数を見ながら探す感じですね。 {face: yes}
zundamon: そのとおりなのだ。目的のピースは長くても1｜MiB《メビバイト》だから、
zundamon: あとはその中で改行を高速に探せば行の頭がわかるのだ。
zundamon: 全部の行の位置を一覧にして持たないから、1億行のファイルでもメモリをほとんど使わないのだ。 {face: smile_left}

# Scene@ちょっと休憩

- bg: data/bg/ConcentrationLines1_Green.jpg
- transition: zoom 0.4
- id: zundamon@smile_left_closeup {fx: shake(6, 20)}
- text: ずんだ餅も「ピース」で数えるのだ {y: -380, size: 72, color: #ffffff, outline: 6 #2e7d32, at: L1, in: pop 0.4, layer: 60}

## Lines
zundamon: ちなみにずんだ餅も、1万個あったら木にして管理したいのだ！ {motion: bounce(40), se: data/sound/boyon1.mp3}
tumugi: 先輩、それは管理しないで、みんなで分けて食べましょう。 {se: data/sound/tsukkomi-1.mp3}

# Scene@3. 編集

- bg: data/yyeditor_large_file/bg/08_pathcopy.svg
- transition: zoom 0.4
- id: zundamon@default_left {x: -790, y: 330, scale: 0.72}
- id: tumugi@default {x: 790, y: 330, scale: 0.72}

## Lines
tumugi: 編集したら、その大きな木を全部作り直すんですか？
zundamon: そこが「永続」の木のすごいところなのだ。
zundamon: 編集したピースまでの通り道、
zundamon: 根っこから葉っぱまでの数個のノードだけを新しく作って、残りは前の版と共有するのだ。
tumugi: 前の版の木も、そのまま残ってるんですか？
zundamon: そうなのだ。だから「ある時点の文書」をスナップショットとして取っておくのが、
zundamon: ほぼタダでできるのだ。

# Scene@スナップ

- bg: data/yyeditor_large_file/bg/09_snapshot.svg
- id: zundamon@default_left {x: -790, y: 330, scale: 0.72}
- id: tumugi@default {x: 790, y: 330, scale: 0.72}

## Lines
zundamon: スナップショットは、木の根っこへの参照を一つコピーするだけなのだ。
zundamon: 元に戻す、つまりアンドゥは、編集の前と後のスナップショットを積んでおくだけで実現しているのだ。
tumugi: じゃあ、10｜GB《ギガバイト》のファイルで「すべて置換」しても……
zundamon: 元に戻すのは一瞬なのだ！ {face: smile_left, se: data/sound/correct1.mp3}
zundamon: それに、検索や保存は裏側で古いスナップショットを読むから、
zundamon: その間もつむぎは文字を打ち続けられるのだ。
tumugi: 待たされないのは、うれしいです！ {face: smile}

# Scene@4. 行数

- bg: data/yyeditor_large_file/bg/10_indexer.svg
- transition: wipe-up 0.5
- id: zundamon@default_left {x: -790, y: 330, scale: 0.72}
- id: tumugi@default {x: 790, y: 330, scale: 0.72}

## Lines
tumugi: でも先輩、ファイル全体の行数って、全部読まないとわからないですよね？
zundamon: そのとおりなのだ。だから開いた直後は、各ピースの改行の数を「まだわからない」にしておくのだ。
zundamon: そして裏側で、ピースの列を最大4つに分けて、別々のスレッドで並列に改行を数えるのだ。
zundamon: 64｜MiB《メビバイト》数えるか、50ミリ秒たつたびに、まとめて画面側に知らせて、木の合計を更新するのだ。
tumugi: 数え終わる前に、行番号を聞かれたらどうするんですか？ {face: komatta}
zundamon: 数え終わったところの「1バイトあたりの改行の多さ」から、残りの行数を推定するのだ。
zundamon: 推定した値だってこともちゃんと区別して持っているから、ごまかしているわけじゃないのだ。 {face: smile_left}
tumugi: 天気予報みたいに、だんだん正確になっていくんですね。

# Scene@4. 変換

- bg: data/yyeditor_large_file/bg/11_transcode.svg
- id: zundamon@default_left {x: -790, y: 330, scale: 0.72}
- id: tumugi@default {x: 790, y: 330, scale: 0.72}

## Lines
tumugi: 古いシステムのファイルだと、シフトジスのこともあるんですけど、それも大丈夫ですか？
zundamon: 中身は全部｜UTF-8《ユーティーエフエイト》で持つ決まりだから、ほかの文字コードは変換が必要なのだ。
zundamon: 大きいファイルは、裏側で4｜MiB《メビバイト》ずつ変換して、一時ファイルに書き出すのだ。
zundamon: それをまたメモリマップするから、変換している間も、先頭のほうは読めるのだ。
tumugi: 改行が全然ないファイルはどうですか？ 1行が何ギガもあるような。
zundamon: そういう長い行も、画面に出すときは8キロバイトくらいずつに区切って並べるのだ。
zundamon: だから横に長すぎても固まらないのだ。

# Scene@保存

- bg: data/yyeditor_large_file/bg/12_save.svg
- transition: slide-left 0.5
- id: zundamon@default_left {x: -790, y: 330, scale: 0.72}
- id: tumugi@default {x: 790, y: 330, scale: 0.72}

## Lines
tumugi: 最後に保存ですけど、元のファイルを見ながら、そのファイルに上書きってできるんですか？
zundamon: ここで、さっきの「削除はいい」という鍵が効いてくるのだ。
zundamon: まず同じフォルダーの一時ファイルに、新しい中身を書き出すのだ。
zundamon: 1｜MiB《メビバイト》ずつ書くから、途中でも中止できるのだ。
zundamon: 書き終わったら、元のファイルを別の名前に逃がして、一時ファイルを本来の名前に付け替えるのだ。
zundamon: 逃がしたファイルは消しちゃうけど、
zundamon: メモリマップはその中身を見続けられるから、編集中の画面も壊れないのだ。
tumugi: 途中で失敗したら？
zundamon: 付け替えに失敗したら、逃がしたファイルを元の名前に戻すのだ。大事なファイルを失わない作りなのだ。 {face: smile_left}

# Scene@まとめ

- bg: data/yyeditor_large_file/bg/13_summary.svg
- transition: fade 0.8
- id: zundamon@smile_left {x: -790, y: 330, scale: 0.72}
- id: tumugi@smile {x: 790, y: 330, scale: 0.72}

## Lines
zundamon: まとめるのだ！ 読まずにメモリマップで見るだけ。元のファイルは書き換えずに、ピースで編集を表す。
zundamon: ピースは永続｜B+木《ビープラスぎ》に並べて、速く探して、スナップショットはほぼタダ。時間のかかる仕事は裏でやる。
tumugi: 「やらなくていい仕事は、やらない」ですね！
zundamon: そのとおりなのだ。だから10｜GB《ギガバイト》のファイルでも、開いた瞬間からスクロールも編集もできるのだ！ {motion: bounce(30), se: data/sound/kira2.mp3}
tumugi: あーしのログファイルも、さっそく開いてみます！

# Scene@エンディング

- bg: data/bg/PC_Room_Daytime.jpg
- transition: fade 0.6
- length: 3
- id: zundamon@smile_left
- id: tumugi@smile
- text: ご視聴ありがとうございました {y: -300, size: 80, color: #ffe000, outline: 6 #000000 | 12 #ffffff, at: L1, in: pop 0.5, layer: 60}

## Lines
zundamon: 最後まで見てくれてありがとうなのだ。巨大ファイルが来ても、もうこわくないのだ！ {wait: 0.5}
tumugi: ありがとうございました！
