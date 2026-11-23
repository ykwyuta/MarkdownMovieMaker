# Scene

- bg: data/civil_low_fraud/bg/001.png
- before_pause: 2
- after_pause: 2
- speed: 1.0
- id: zundamon@voice_only

## Lines
zundamon@noscript: 詐欺による契約

# Scene@導入 詐欺による意思表示

- bg: data/bg/Blackboard.png
- id: zundamon@default_left_closeup
- id: metan@default
- id: tumugi@default

## Lines
zundamon: 今日のテーマは「詐欺による契約」なのだ

# Scene@導入 詐欺による意思表示

- id: zundamon@default_left
- id: metan@default_closeup
- id: tumugi@default

## Lines
metan: 「詐欺による契約」つまり、騙されてした契約ね

# Scene@導入 詐欺による意思表示

- id: zundamon@default_left
- id: metan@default_closeup
- id: tumugi@default

## Lines
metan: 詐欺による契約は、無効（最初からなかったことになる）なのではなく、有効に成立しているけど、後から「取り消す」ことができるとされているの
metan: なぜ錯誤（勘違い）と違って有効と考えるかというと、騙されたとしても、「買おう」と思った意思と、「買います」と表示した行為が一致しているからよ。意思表示の基本は一致している、というわけね
metan: 取り消しができる期間は、追認ができるようになってから5年、かつ、行為のときから20年以内よ

# Scene@導入 詐欺による意思表示

- id: zundamon@default_left
- id: metan@default
- id: tumugi@default_closeup

## Lines
tumugi: 「だまされたけど、買うぞ！」って思って行動したんだから、一旦は契約が成立します
tumugi: でも、やっぱりだまされたのはかわいそうだから、後で「なかったことにして！」って言えるチャンスがある、ってイメージすると分かりやすいですよ

# Scene

- bg: data/civil_low_fraud/bg/002.png
- before_pause: 2
- after_pause: 2
- speed: 1.0
- id: tumugi@voice_only

## Lines
tumugi@noscript: 第三者による詐欺

# Scene@第三者詐欺

- bg: data/bg/Blackboard.png
- id: zundamon@default_left_closeup
- id: metan@default
- id: tumugi@default

## Lines
zundamon: もし、僕がAさんで、Cさんっていう別の人に騙されて、Bさんと契約しちゃった場合はどうなるのだ？
zundamon: Bさんが何も知らなかったら、僕の契約は取り消せないのかな？

# Scene@第三者詐欺

- id: zundamon@default_left
- id: metan@default_closeup
- id: tumugi@default

## Lines
metan: いい質問ね。それを第三者詐欺と言うわ
metan: 契約の相手方Bの状況 善意無過失（知らず、知ることもできなかった）。結果 AはBとの契約を取り消せない。誰が保護されるか 契約の相手方B
metan: 契約の相手方Bの状況 悪意（知っていた）または有過失（知ることができた）。結果 AはBとの契約を取り消せる。誰が保護されるか 騙されたA
metan: 契約の相手方であるBが何も悪くなかった場合（善意無過失）は、Bが保護されて、Aは契約を取り消すことができないのよ

# Scene@第三者詐欺

- bg: data/bg/Blackboard.png
- after_pause: 2
- id: zundamon@default_left
- id: metan@default
- id: tumugi@default
- overlay: data/overlay/fraud002.png 

## Lines
tumugi: Bさんが無関係なのに、Aさんの契約が勝手に取り消されちゃうのは可哀想ですよね
tumugi: だから、Bさんが悪くなければ、Bさんが守られるんです。「Bさんが悪かったか？」で判断するんですね

# Scene

- bg: data/civil_low_fraud/bg/003.png
- before_pause: 2
- after_pause: 2
- speed: 1.0
- id: tumugi@voice_only

## Lines
tumugi@noscript: 詐欺取消しと第三者

# Scene@詐欺取消しと第三者

- bg: data/bg/Blackboard.png
- id: zundamon@default_left_closeup
- id: metan@default
- id: tumugi@default

## Lines
zundamon: じゃあ、もしBさんが、僕との契約を取り消す前に、誰か別の人（Dさん）にその土地を売っちゃったら、どうなるのだ？

# Scene@詐欺取消し「前」の第三者

- id: zundamon@default_left
- id: metan@default_closeup
- id: tumugi@default

## Lines
metan: それは詐欺取消し「前」の第三者との対抗問題ね
metan: AがBに騙されて売却 → BがDに転売 → Aが詐欺を理由に契約を取り消した
metan: この場合、Dが詐欺の事実について善意無過失であれば、Dが保護されて、AはDに対して所有権を主張できないわ
metan: もしDが悪意または有過失であれば、Aが保護され、AはDから土地を取り戻せるわ

# Scene@詐欺取消し「前」の第三者

- id: zundamon@default_left
- id: metan@default
- id: tumugi@default
- overlay: data/overlay/fraud001.png 

## Lines
tumugi: この場合のDさんは、AさんとBさんの間で契約が「有効なまま」だったときに土地を買ったんです
tumugi: だから、Dさんが「だまされたことなんて知らなかったし、知ることもできなかったよ！」っていう善意無過失なら、Dさんを優先するんですね！ 

# Scene@詐欺取消し「後」の第三者

- id: zundamon@default_left_closeup
- id: metan@default
- id: tumugi@default

## Lines
zundamon: じゃあ、もし僕がBさんとの契約を取り消した後に、Bさんが勝手にDさんに売っちゃった場合はどうなるのだ？

# Scene@詐欺取消し「後」の第三者

- id: zundamon@default_left
- id: metan@default_closeup
- id: tumugi@default

## Lines
metan: その場合は、二重譲渡と同じ考え方になるのよ
metan: Aが契約を取り消すと、契約は契約時にさかのぼって無効になる
metan: 所有権はBからAに自動的に戻っていることになる
metan: しかし、BはそれをDにも売っているため、「AとDの二人に所有権が渡った」という二重譲渡の関係になるの
metan: この場合、先に法務局で登記（対抗要件）を備えた方が、自分の権利を主張できるわ

# Scene@詐欺取消し「後」の第三者

- id: zundamon@default_left
- id: metan@default
- id: tumugi@default
- overlay: data/overlay/fraud001.png 

## Lines
tumugi: そうそう！これはシンプルです。取り消しをして所有権がAさんに戻った（登記なし）後、BさんがDさんに売った（Dさんが登記した）場合、先に登記をした人が勝ち！ってことです
tumugi: AさんもDさんも登記を頑張って、勝った方が権利を主張できるってことですね

# Scene

- bg: data/civil_low_fraud/bg/004.png
- before_pause: 2
- after_pause: 2
- speed: 1.0
- id: zundamon@voice_only

## Lines
zundamon@noscript: 強迫との違い

# Scene@比較ポイント 強迫との違い

- bg: data/bg/Blackboard.png
- id: zundamon@default_left
- id: metan@default_closeup
- id: tumugi@default

## Lines
metan: 最後に、詐欺と似ているけど違う強迫と比較して覚えておくといいわ
metan: 善意無過失の第三者との関係で、詐欺による取消しは対抗できない（第三者が保護される）けれど、強迫による取消しは対抗できる
metan: 強迫は、騙しとは比べ物にならないくらい悪質性が高いから、脅された人が絶対的に保護されて、善意無過失の第三者に対しても契約の取消しを主張できるのよ

# Scene@比較ポイント 強迫との違い

- after_pause: 2
- id: zundamon@default_left
- id: metan@default
- id: tumugi@default
- overlay: data/overlay/fraud003.png 

## Lines
zundamon: 強迫の方が、第三者がいても「取り消せる！」ってことなのだ

# Scene

- bg: data/civil_low_fraud/bg/004.png
- speed: 1.0
- id: zundamon@voice_only

## Lines
zundamon@noscript: まとめ

# Scene@まとめ

- id: zundamon@default_left
- id: metan@default
- id: tumugi@default
- overlay: data/overlay/fraud004.png 

## Lines
zundamon@noscript: 詐欺はだますことなのだ！契約は有効だけど、後から取り消せるのだ
zundamon@noscript: 取消しできる期間は追認できる時から5年、または行為から20年以内なのだ
zundamon@noscript: 第三者詐欺の場合、AがCに騙されてBと契約した場合、Bが「善意無過失」なら、Aは取り消せないのだ！ Bが守られるのだ
zundamon@noscript: 取消し「前」の第三者の場合、契約を取り消す前に現れた第三者Dが「善意無過失」なら、AはDに対抗できないのだ！Dが守られるのだ
zundamon@noscript: 取消し「後」の第三者の場合、契約を取り消した後に現れた第三者Dとは、二重譲渡と同じ考え方なのだ！先に登記をした方が勝ちなのだ
zundamon@noscript: 強迫は詐欺より悪質だから、善意無過失の第三者に対しても、取り消しを対抗できるのだ

# Scene

- bg: data/civil_low_fraud/bg/004.png
- speed: 1.0
- id: zundamon@voice_only

## Lines
zundamon@noscript: またみてね

# Scene

- after_pause: 2
- sound: data/sound/chan-chan1.mp3 1