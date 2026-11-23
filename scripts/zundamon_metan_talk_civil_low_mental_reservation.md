# Scene

- bg: data/civil_low_mental_reservation/bg/001.png
- before_pause: 2
- after_pause: 2
- speed: 1.0
- id: tumugi@voice_only

## Lines
tumugi@noscript: 心裡留保って何？

# Scene@導入 心裡留保って何？

- bg: data/bg/Blackboard.png
- id: zundamon@default_left_closeup
- id: metan@default
- id: tumugi@default

## Lines
zundamon: 心裡留保って、なんだか難しそうな言葉なのだ！ まずは心裡留保の言葉の意味から教えてほしいのだ

# Scene@導入 心裡留保って何？

- id: zundamon@default_left
- id: metan@default_closeup
- id: tumugi@default

## Lines
metan: わかりました、ずんだもん。心裡留保というのは、簡単に言うと「冗談」や「本心ではない」意思表示のことです。
metan: 法律用語では「表意者が真意でないことを知りながら行った意思表示」と説明されます。

# Scene@導入 心裡留保って何？

- id: zundamon@default_left
- id: metan@default
- id: tumugi@default_closeup

## Lines
tumugi: 「心の中で思っていることと、口から出た言葉が違う」ってことですね！
tumugi: 心裡留保の「留保」って言葉が、心の中に本心を留めておくイメージですね。

# Scene

- bg: data/civil_low_mental_reservation/bg/002.png
- before_pause: 2
- after_pause: 2
- speed: 1.0
- id: tumugi@voice_only

## Lines
tumugi@noscript: 当事者間の効力 冗談の契約は有効？無効？

# Scene@当事者間の効力 冗談の契約は有効？無効？

- bg: data/bg/Blackboard.png
- id: zundamon@default_left_closeup
- id: metan@default
- id: tumugi@default

## Lines
zundamon: それじゃあ、冗談で「土地をあげる」って言っちゃった場合、その契約は有効になるの？
zundamon: それとも無効になるの？
zundamon: 僕が冗談を言った本人の「表意者」だとして、めたんが「相手方」だったら、どうなるのだ？

# Scene@当事者間の効力 冗談の契約は有効？無効？

- overlay: data/overlay/table_contract_effect.png 
- id: zundamon@default_left
- id: metan@default_closeup
- id: tumugi@default

## Lines
metan: そこが心裡留保のポイントです。基本的には、冗談を言った表意者（ずんだもん）が悪いのですが、相手方（私）の状況によって結論が変わってきます。
metan: 相手方 B の状態：悪意（冗談だと知っていた） 契約の効力：無効
metan: 理由：相手方を保護する必要がない。
metan: 相手方 B の状態：有過失（知ることができた不注意があった） 契約の効力：無効
metan: 理由：相手方に落ち度があるため、保護する必要がない。
metan: 相手方 B の状態：善意無過失（冗談だと知らず、知ることもできなかった） 契約の効力：有効
metan: 理由：相手方を守る（取引の安全を優先する）必要性が高いため。
metan: つまり、私（相手方）が「善意無過失」、つまり、「ずんだもんが冗談だと全く知らず、注意しても知ることができなかった」場合に限り、契約は有効になります。
metan: それ以外の場合は、無効となるのですよ。

# Scene@当事者間の効力 冗談の契約は有効？無効？

- bg: data/bg/Blackboard.png
- after_pause: 2
- id: zundamon@default_left
- id: metan@default
- id: tumugi@default_closeup

## Lines
tumugi: 「冗談だと知らなかった、知るすべもなかった人だけが守られる」ってイメージを持てばいいですよ！
tumugi: それ以外の場合は、冗談を言った人が「やっぱり冗談でした！」って主張して、契約を無効にできちゃうってことですね！

# Scene

- bg: data/civil_low_mental_reservation/bg/003.png
- before_pause: 2
- after_pause: 2
- speed: 1.0
- id: tumugi@voice_only

## Lines
tumugi@noscript: 第三者の関係 土地は取り戻せる？

# Scene@第三者の関係 土地は取り戻せる？

- bg: data/bg/Blackboard.png
- id: zundamon@default_left_closeup
- id: metan@default
- id: tumugi@default

## Lines
zundamon: じゃあ、もし無効になった後で、めたんがその土地を、つむぎに売っちゃったらどうなるのだ？
zundamon: 僕（表意者）は、つむぎ（第三者）から土地を取り戻せるの？

# Scene@第三者の関係 土地は取り戻せる？

- overlay: data/overlay/table_contract_effect2.png 
- id: zundamon@default_left
- id: metan@default_closeup
- id: tumugi@default

## Lines
metan: これも重要な論点です。心裡留保によって当事者間の契約が「無効」になった場合でも、その無効を「善意の第三者」には主張できません。
metan: 第三者 C の状態：善意（心裡留保の事実を知らなかった）。A は C に無効を主張できるか：主張できない (対抗できない) 
metan: 理由：第三者を保護し、取引の安全を図るため。
metan: 第三者 C の状態：悪意（心裡留保の事実を知っていた）。 A は C に無効を主張できるか：主張できる (対抗できる)  
metan: 理由：悪意の第三者を保護する必要はない。

# Scene@第三者の関係 土地は取り戻せる？

- after_pause: 2
- id: zundamon@default_left
- id: metan@default
- id: tumugi@default_closeup

## Lines
tumugi: この「善意の第三者には対抗できない」ってルール、超大事です！
tumugi: ちなみに、この第三者が守られるためには、「善意」だけでOKで、「無過失」まではいらないんですよ！
tumugi: めたん先輩から買ったつむぎ（第三者）が、ちょっと不注意（有過失）だったとしても、冗談だと知らなかった（善意）なら、ずんだもん先輩（表意者）は土地を取り戻せないってことです！
tumugi: この点、当事者間の「善意無過失」とは違うから、注意が必要ですね！


# Scene@第三者の関係 土地は取り戻せる？

- id: zundamon@default_left
- id: metan@default
- id: tumugi@default

## Lines
zundamon: 僕が冗談を言ったばっかりに、土地が無効になったり有効になったり、さらに第三者のつむぎに取られちゃったりするなんて、大変なのだ！ 冗談もほどほどにするのだ…

# Scene@第三者の関係 土地は取り戻せる？

- after_pause: 2
- id: zundamon@default_left
- id: metan@default
- id: tumugi@default

## Lines
metan: そうですね。民法は、真意と異なる意思表示でも、相手方や第三者の信頼をどこまで守るかを定めているのですよ。
metan: ずんだもんの例のように、軽はずみな冗談が大きな結果を招くこともあるので、注意が必要ですね。

# Scene

- bg: data/civil_low_mental_reservation/bg/004.png
- speed: 1.0
- id: zundamon@voice_only

## Lines
zundamon@noscript: またみてね

# Scene

- after_pause: 2
- sound: data/sound/chan-chan1.mp3 1
