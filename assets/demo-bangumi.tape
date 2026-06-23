# bilidown 番剧（EP/SS）demo
# 用法（在仓库根目录）：vhs assets/demo-bangumi.tape
# 产物：assets/demo-bangumi.gif
# 提示：下载耗时随网速变化，必要时调大 download 后的 Sleep；想更快可在命令里加 --audio-only。

Output assets/demo-bangumi.gif

Set Shell "zsh"
Set FontSize 20
Set Width 1200
Set Height 720
Set Padding 18
Set TypingSpeed 60ms

Hide
Type "export PATH=$HOME/.cargo/bin:$PATH; clear"
Enter
Show

Type "bilidown info ss241775"
Sleep 600ms
Enter
Sleep 4s

Type "bilidown download ep4350988 --quality 16 -o /tmp/bilidemo"
Sleep 600ms
Enter
Sleep 22s

Sleep 2s
