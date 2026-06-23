# bilidown 搜索 → 下载 demo
# 用法（在仓库根目录）：vhs assets/demo-search.tape
# 产物：assets/demo-search.gif
# 提示：下载耗时随网速变化；若 gif 在"下载完成"前就结束，调大下面 download 后的 Sleep。

Output assets/demo-search.gif

Set Shell "zsh"
Set FontSize 20
Set Width 1200
Set Height 640
Set Padding 18
Set TypingSpeed 60ms

# off-camera：把 bilidown 所在的 ~/.cargo/bin 加进 PATH，并清屏
Hide
Type "export PATH=$HOME/.cargo/bin:$PATH; clear"
Enter
Show

Type "bilidown search 编程 -n 5"
Sleep 600ms
Enter
Sleep 3.5s

Type "bilidown download BV1AF411s78P --quality 16 -o /tmp/bilidemo"
Sleep 600ms
Enter
Sleep 20s

Sleep 2s
