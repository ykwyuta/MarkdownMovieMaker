<!-- 拡張構文（YMM4 相当の機能）のデモ。mmm scripts/ymm4_features_demo.md --engine silent で確認できます -->
# Scene@オープニング

- bg: data/bg/Blackboard.png {scale: 1.15->1, fx: blur(6)}
- length: 3
- shape: rect {w: 1920, h: 260, color: #000000a0, y: 0, in: wipe-left 0.6, layer: 30}
- text: "MarkdownMovieMaker 拡張構文デモ" {size: 110, color: #ffe000, outline: 6 #000000 | 12 #ffffff, shadow: 6 6 8 #00000099, in: pop 0.6, layer: 40}
- text: YMM4 でできることを台本だけで {y: 110, size: 48, color: white, at: 0.8, in: fade 0.5, typewriter: 0.05, layer: 40}
- bgm: data/bgm/001.mp3 {volume: 0.15, fade_in: 1, continue}
- sound: data/sound/presentation-title1.mp3 {at: 0.2}

# Scene@立ち絵・表情・動き

- bg: data/bg/Blackboard.png
- transition: wipe-left 0.6
- id: zundamon@default_left {in: slide-left 0.5}
- id: metan@default {in: slide-right 0.5, x: 520}
- image: data/overlay/business_kaitataku.png {scale: 60%, y: -250, at: L2, in: zoom 0.4, out: fade 0.3, fx: shadow(10, 10, 12) , rotate: -4}

## Lines
zundamon: 立ち絵はスライドして登場できるのだ！ {motion: bounce(30, 6)}
metan@komatta: ｜表情《ひょうじょう》はセリフごとに変えられますわ。 {wait: 0.4}
zundamon: 声の高さや音量も変えられるのだ {pitch: 1.15, volume: 1.1, se: data/sound/pafu1.mp3}

# Scene@カメラと場面切り替え

- bg: data/bg/Blackboard.png
- transition: circle 0.8
- camera: {zoom: 1->1.25 ease-in-out, x: 0->-120}
- id: zundamon@default_left
- id: tumugi@default_closeup {fx: shake(6, 20)}
- shape: star {w: 220, h: 220, color: #ffd700, stroke: 6 #ffffff, x: 600, y: -300, fx: spin(120) glow(16, #fff3a0), layer: 50}

## Lines
tumugi: カメラでズームしたり、図形を回したりもできます。
zundamon: 場面切り替えも円やワイプなどから選べるのだ。 {motion: shake(4)}

# Scene@エフェクト

- bg: data/bg/Blackboard.png {fx: sepia}
- transition: black 0.8
- id: metan@default {fx: outline(6, #ffffff) shadow(12, 12, 16)}
- id: zundamon@default_left {fx: grayscale, opacity: 0.8}
- text: 映像エフェクト {x: -500, y: -380, size: 72, color: white, box: #c03030e0 24 18, in: drop 0.6 bounce}

## Lines
metan: 縁取り・影・モノクロ・セピアなどの映像エフェクトを重ねられます。

# Scene@ぱりーん

- bg: data/bg/Blackboard.png
- transition: shatter 1.2
- length: 4
- id: zundamon@default_left {out: shatter 1.0}
- image: data/overlay/fraud001.png {scale: 70%, y: -180, at: 0.8, until: 3.2, out: shatter 0.9}
- text: ぱりーん！ {size: 140, color: #ffffff, outline: 8 #3060ff, at: 2.2, in: pop 0.3, layer: 120}

## Lines
metan: 割れて飛び散る演出もできますわ。 {wait: 1.5}
