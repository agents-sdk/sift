# 多文件 git diff

保留文件头和关键 hunk，抽稀上下文行，完整 diff 写入 CCR。

- 场景 ID：`git-diff`
- 检测类型：`git_diff`
- 相关性 query：`new_line`

## 压缩前原文

> 内容完整保留；为了便于阅读，JSON 仅在展示时进行了缩进美化。

```text
diff --git a/src/file0.rs b/src/file0.rs
--- a/src/file0.rs
+++ b/src/file0.rs
@@ -0,7 +0,7 @@ fn region_0
 context_0_0_0
 context_0_0_1
 context_0_0_2
-    old_line_0_0
+    new_line_0_0
 context_0_0_4
 context_0_0_5
 context_0_0_6
@@ -10,7 +10,7 @@ fn region_1
 context_0_1_0
 context_0_1_1
 context_0_1_2
-    old_line_0_1
+    new_line_0_1
 context_0_1_4
 context_0_1_5
 context_0_1_6
@@ -20,7 +20,7 @@ fn region_2
 context_0_2_0
 context_0_2_1
 context_0_2_2
-    old_line_0_2
+    new_line_0_2
 context_0_2_4
 context_0_2_5
 context_0_2_6
@@ -30,7 +30,7 @@ fn region_3
 context_0_3_0
 context_0_3_1
 context_0_3_2
-    old_line_0_3
+    new_line_0_3
 context_0_3_4
 context_0_3_5
 context_0_3_6
@@ -40,7 +40,7 @@ fn region_4
 context_0_4_0
 context_0_4_1
 context_0_4_2
-    old_line_0_4
+    new_line_0_4
 context_0_4_4
 context_0_4_5
 context_0_4_6
@@ -50,7 +50,7 @@ fn region_5
 context_0_5_0
 context_0_5_1
 context_0_5_2
-    old_line_0_5
+    new_line_0_5
 context_0_5_4
 context_0_5_5
 context_0_5_6
diff --git a/src/file1.rs b/src/file1.rs
--- a/src/file1.rs
+++ b/src/file1.rs
@@ -100,7 +100,7 @@ fn region_0
 context_1_0_0
 context_1_0_1
 context_1_0_2
-    old_line_1_0
+    new_line_1_0
 context_1_0_4
 context_1_0_5
 context_1_0_6
@@ -110,7 +110,7 @@ fn region_1
 context_1_1_0
 context_1_1_1
 context_1_1_2
-    old_line_1_1
+    new_line_1_1
 context_1_1_4
 context_1_1_5
 context_1_1_6
@@ -120,7 +120,7 @@ fn region_2
 context_1_2_0
 context_1_2_1
 context_1_2_2
-    old_line_1_2
+    new_line_1_2
 context_1_2_4
 context_1_2_5
 context_1_2_6
@@ -130,7 +130,7 @@ fn region_3
 context_1_3_0
 context_1_3_1
 context_1_3_2
-    old_line_1_3
+    new_line_1_3
 context_1_3_4
 context_1_3_5
 context_1_3_6
@@ -140,7 +140,7 @@ fn region_4
 context_1_4_0
 context_1_4_1
 context_1_4_2
-    old_line_1_4
+    new_line_1_4
 context_1_4_4
 context_1_4_5
 context_1_4_6
@@ -150,7 +150,7 @@ fn region_5
 context_1_5_0
 context_1_5_1
 context_1_5_2
-    old_line_1_5
+    new_line_1_5
 context_1_5_4
 context_1_5_5
 context_1_5_6
diff --git a/src/file2.rs b/src/file2.rs
--- a/src/file2.rs
+++ b/src/file2.rs
@@ -200,7 +200,7 @@ fn region_0
 context_2_0_0
 context_2_0_1
 context_2_0_2
-    old_line_2_0
+    new_line_2_0
 context_2_0_4
 context_2_0_5
 context_2_0_6
@@ -210,7 +210,7 @@ fn region_1
 context_2_1_0
 context_2_1_1
 context_2_1_2
-    old_line_2_1
+    new_line_2_1
 context_2_1_4
 context_2_1_5
 context_2_1_6
@@ -220,7 +220,7 @@ fn region_2
 context_2_2_0
 context_2_2_1
 context_2_2_2
-    old_line_2_2
+    new_line_2_2
 context_2_2_4
 context_2_2_5
 context_2_2_6
@@ -230,7 +230,7 @@ fn region_3
 context_2_3_0
 context_2_3_1
 context_2_3_2
-    old_line_2_3
+    new_line_2_3
 context_2_3_4
 context_2_3_5
 context_2_3_6
@@ -240,7 +240,7 @@ fn region_4
 context_2_4_0
 context_2_4_1
 context_2_4_2
-    old_line_2_4
+    new_line_2_4
 context_2_4_4
 context_2_4_5
 context_2_4_6
@@ -250,7 +250,7 @@ fn region_5
 context_2_5_0
 context_2_5_1
 context_2_5_2
-    old_line_2_5
+    new_line_2_5
 context_2_5_4
 context_2_5_5
 context_2_5_6
diff --git a/src/file3.rs b/src/file3.rs
--- a/src/file3.rs
+++ b/src/file3.rs
@@ -300,7 +300,7 @@ fn region_0
 context_3_0_0
 context_3_0_1
 context_3_0_2
-    old_line_3_0
+    new_line_3_0
 context_3_0_4
 context_3_0_5
 context_3_0_6
@@ -310,7 +310,7 @@ fn region_1
 context_3_1_0
 context_3_1_1
 context_3_1_2
-    old_line_3_1
+    new_line_3_1
 context_3_1_4
 context_3_1_5
 context_3_1_6
@@ -320,7 +320,7 @@ fn region_2
 context_3_2_0
 context_3_2_1
 context_3_2_2
-    old_line_3_2
+    new_line_3_2
 context_3_2_4
 context_3_2_5
 context_3_2_6
@@ -330,7 +330,7 @@ fn region_3
 context_3_3_0
 context_3_3_1
 context_3_3_2
-    old_line_3_3
+    new_line_3_3
 context_3_3_4
 context_3_3_5
 context_3_3_6
@@ -340,7 +340,7 @@ fn region_4
 context_3_4_0
 context_3_4_1
 context_3_4_2
-    old_line_3_4
+    new_line_3_4
 context_3_4_4
 context_3_4_5
 context_3_4_6
@@ -350,7 +350,7 @@ fn region_5
 context_3_5_0
 context_3_5_1
 context_3_5_2
-    old_line_3_5
+    new_line_3_5
 context_3_5_4
 context_3_5_5
 context_3_5_6
diff --git a/src/file4.rs b/src/file4.rs
--- a/src/file4.rs
+++ b/src/file4.rs
@@ -400,7 +400,7 @@ fn region_0
 context_4_0_0
 context_4_0_1
 context_4_0_2
-    old_line_4_0
+    new_line_4_0
 context_4_0_4
 context_4_0_5
 context_4_0_6
@@ -410,7 +410,7 @@ fn region_1
 context_4_1_0
 context_4_1_1
 context_4_1_2
-    old_line_4_1
+    new_line_4_1
 context_4_1_4
 context_4_1_5
 context_4_1_6
@@ -420,7 +420,7 @@ fn region_2
 context_4_2_0
 context_4_2_1
 context_4_2_2
-    old_line_4_2
+    new_line_4_2
 context_4_2_4
 context_4_2_5
 context_4_2_6
@@ -430,7 +430,7 @@ fn region_3
 context_4_3_0
 context_4_3_1
 context_4_3_2
-    old_line_4_3
+    new_line_4_3
 context_4_3_4
 context_4_3_5
 context_4_3_6
@@ -440,7 +440,7 @@ fn region_4
 context_4_4_0
 context_4_4_1
 context_4_4_2
-    old_line_4_4
+    new_line_4_4
 context_4_4_4
 context_4_4_5
 context_4_4_6
@@ -450,7 +450,7 @@ fn region_5
 context_4_5_0
 context_4_5_1
 context_4_5_2
-    old_line_4_5
+    new_line_4_5
 context_4_5_4
 context_4_5_5
 context_4_5_6
diff --git a/src/file5.rs b/src/file5.rs
--- a/src/file5.rs
+++ b/src/file5.rs
@@ -500,7 +500,7 @@ fn region_0
 context_5_0_0
 context_5_0_1
 context_5_0_2
-    old_line_5_0
+    new_line_5_0
 context_5_0_4
 context_5_0_5
 context_5_0_6
@@ -510,7 +510,7 @@ fn region_1
 context_5_1_0
 context_5_1_1
 context_5_1_2
-    old_line_5_1
+    new_line_5_1
 context_5_1_4
 context_5_1_5
 context_5_1_6
@@ -520,7 +520,7 @@ fn region_2
 context_5_2_0
 context_5_2_1
 context_5_2_2
-    old_line_5_2
+    new_line_5_2
 context_5_2_4
 context_5_2_5
 context_5_2_6
@@ -530,7 +530,7 @@ fn region_3
 context_5_3_0
 context_5_3_1
 context_5_3_2
-    old_line_5_3
+    new_line_5_3
 context_5_3_4
 context_5_3_5
 context_5_3_6
@@ -540,7 +540,7 @@ fn region_4
 context_5_4_0
 context_5_4_1
 context_5_4_2
-    old_line_5_4
+    new_line_5_4
 context_5_4_4
 context_5_4_5
 context_5_4_6
@@ -550,7 +550,7 @@ fn region_5
 context_5_5_0
 context_5_5_1
 context_5_5_2
-    old_line_5_5
+    new_line_5_5
 context_5_5_4
 context_5_5_5
 context_5_5_6
diff --git a/src/file6.rs b/src/file6.rs
--- a/src/file6.rs
+++ b/src/file6.rs
@@ -600,7 +600,7 @@ fn region_0
 context_6_0_0
 context_6_0_1
 context_6_0_2
-    old_line_6_0
+    new_line_6_0
 context_6_0_4
 context_6_0_5
 context_6_0_6
@@ -610,7 +610,7 @@ fn region_1
 context_6_1_0
 context_6_1_1
 context_6_1_2
-    old_line_6_1
+    new_line_6_1
 context_6_1_4
 context_6_1_5
 context_6_1_6
@@ -620,7 +620,7 @@ fn region_2
 context_6_2_0
 context_6_2_1
 context_6_2_2
-    old_line_6_2
+    new_line_6_2
 context_6_2_4
 context_6_2_5
 context_6_2_6
@@ -630,7 +630,7 @@ fn region_3
 context_6_3_0
 context_6_3_1
 context_6_3_2
-    old_line_6_3
+    new_line_6_3
 context_6_3_4
 context_6_3_5
 context_6_3_6
@@ -640,7 +640,7 @@ fn region_4
 context_6_4_0
 context_6_4_1
 context_6_4_2
-    old_line_6_4
+    new_line_6_4
 context_6_4_4
 context_6_4_5
 context_6_4_6
@@ -650,7 +650,7 @@ fn region_5
 context_6_5_0
 context_6_5_1
 context_6_5_2
-    old_line_6_5
+    new_line_6_5
 context_6_5_4
 context_6_5_5
 context_6_5_6
diff --git a/src/file7.rs b/src/file7.rs
--- a/src/file7.rs
+++ b/src/file7.rs
@@ -700,7 +700,7 @@ fn region_0
 context_7_0_0
 context_7_0_1
 context_7_0_2
-    old_line_7_0
+    new_line_7_0
 context_7_0_4
 context_7_0_5
 context_7_0_6
@@ -710,7 +710,7 @@ fn region_1
 context_7_1_0
 context_7_1_1
 context_7_1_2
-    old_line_7_1
+    new_line_7_1
 context_7_1_4
 context_7_1_5
 context_7_1_6
@@ -720,7 +720,7 @@ fn region_2
 context_7_2_0
 context_7_2_1
 context_7_2_2
-    old_line_7_2
+    new_line_7_2
 context_7_2_4
 context_7_2_5
 context_7_2_6
@@ -730,7 +730,7 @@ fn region_3
 context_7_3_0
 context_7_3_1
 context_7_3_2
-    old_line_7_3
+    new_line_7_3
 context_7_3_4
 context_7_3_5
 context_7_3_6
@@ -740,7 +740,7 @@ fn region_4
 context_7_4_0
 context_7_4_1
 context_7_4_2
-    old_line_7_4
+    new_line_7_4
 context_7_4_4
 context_7_4_5
 context_7_4_6
@@ -750,7 +750,7 @@ fn region_5
 context_7_5_0
 context_7_5_1
 context_7_5_2
-    old_line_7_5
+    new_line_7_5
 context_7_5_4
 context_7_5_5
 context_7_5_6
```

## 压缩后输出

```text
diff --git a/src/file0.rs b/src/file0.rs
--- a/src/file0.rs
+++ b/src/file0.rs
@@ -0,7 +0,7 @@ fn region_0
 context_0_0_1
 context_0_0_2
-    old_line_0_0
+    new_line_0_0
 context_0_0_4
 context_0_0_5
@@ -10,7 +10,7 @@ fn region_1
 context_0_1_1
 context_0_1_2
-    old_line_0_1
+    new_line_0_1
 context_0_1_4
 context_0_1_5
@@ -20,7 +20,7 @@ fn region_2
 context_0_2_1
 context_0_2_2
-    old_line_0_2
+    new_line_0_2
 context_0_2_4
 context_0_2_5
@@ -30,7 +30,7 @@ fn region_3
 context_0_3_1
 context_0_3_2
-    old_line_0_3
+    new_line_0_3
 context_0_3_4
 context_0_3_5
@@ -40,7 +40,7 @@ fn region_4
 context_0_4_1
 context_0_4_2
-    old_line_0_4
+    new_line_0_4
 context_0_4_4
 context_0_4_5
@@ -50,7 +50,7 @@ fn region_5
 context_0_5_1
 context_0_5_2
-    old_line_0_5
+    new_line_0_5
 context_0_5_4
 context_0_5_5
diff --git a/src/file1.rs b/src/file1.rs
--- a/src/file1.rs
+++ b/src/file1.rs
@@ -100,7 +100,7 @@ fn region_0
 context_1_0_1
 context_1_0_2
-    old_line_1_0
+    new_line_1_0
 context_1_0_4
 context_1_0_5
@@ -110,7 +110,7 @@ fn region_1
 context_1_1_1
 context_1_1_2
-    old_line_1_1
+    new_line_1_1
 context_1_1_4
 context_1_1_5
@@ -120,7 +120,7 @@ fn region_2
 context_1_2_1
 context_1_2_2
-    old_line_1_2
+    new_line_1_2
 context_1_2_4
 context_1_2_5
@@ -130,7 +130,7 @@ fn region_3
 context_1_3_1
 context_1_3_2
-    old_line_1_3
+    new_line_1_3
 context_1_3_4
 context_1_3_5
@@ -140,7 +140,7 @@ fn region_4
 context_1_4_1
 context_1_4_2
-    old_line_1_4
+    new_line_1_4
 context_1_4_4
 context_1_4_5
@@ -150,7 +150,7 @@ fn region_5
 context_1_5_1
 context_1_5_2
-    old_line_1_5
+    new_line_1_5
 context_1_5_4
 context_1_5_5
diff --git a/src/file2.rs b/src/file2.rs
--- a/src/file2.rs
+++ b/src/file2.rs
@@ -200,7 +200,7 @@ fn region_0
 context_2_0_1
 context_2_0_2
-    old_line_2_0
+    new_line_2_0
 context_2_0_4
 context_2_0_5
@@ -210,7 +210,7 @@ fn region_1
 context_2_1_1
 context_2_1_2
-    old_line_2_1
+    new_line_2_1
 context_2_1_4
 context_2_1_5
@@ -220,7 +220,7 @@ fn region_2
 context_2_2_1
 context_2_2_2
-    old_line_2_2
+    new_line_2_2
 context_2_2_4
 context_2_2_5
@@ -230,7 +230,7 @@ fn region_3
 context_2_3_1
 context_2_3_2
-    old_line_2_3
+    new_line_2_3
 context_2_3_4
 context_2_3_5
@@ -240,7 +240,7 @@ fn region_4
 context_2_4_1
 context_2_4_2
-    old_line_2_4
+    new_line_2_4
 context_2_4_4
 context_2_4_5
@@ -250,7 +250,7 @@ fn region_5
 context_2_5_1
 context_2_5_2
-    old_line_2_5
+    new_line_2_5
 context_2_5_4
 context_2_5_5
diff --git a/src/file3.rs b/src/file3.rs
--- a/src/file3.rs
+++ b/src/file3.rs
@@ -300,7 +300,7 @@ fn region_0
 context_3_0_1
 context_3_0_2
-    old_line_3_0
+    new_line_3_0
 context_3_0_4
 context_3_0_5
@@ -310,7 +310,7 @@ fn region_1
 context_3_1_1
 context_3_1_2
-    old_line_3_1
+    new_line_3_1
 context_3_1_4
 context_3_1_5
@@ -320,7 +320,7 @@ fn region_2
 context_3_2_1
 context_3_2_2
-    old_line_3_2
+    new_line_3_2
 context_3_2_4
 context_3_2_5
@@ -330,7 +330,7 @@ fn region_3
 context_3_3_1
 context_3_3_2
-    old_line_3_3
+    new_line_3_3
 context_3_3_4
 context_3_3_5
@@ -340,7 +340,7 @@ fn region_4
 context_3_4_1
 context_3_4_2
-    old_line_3_4
+    new_line_3_4
 context_3_4_4
 context_3_4_5
@@ -350,7 +350,7 @@ fn region_5
 context_3_5_1
 context_3_5_2
-    old_line_3_5
+    new_line_3_5
 context_3_5_4
 context_3_5_5
diff --git a/src/file4.rs b/src/file4.rs
--- a/src/file4.rs
+++ b/src/file4.rs
@@ -400,7 +400,7 @@ fn region_0
 context_4_0_1
 context_4_0_2
-    old_line_4_0
+    new_line_4_0
 context_4_0_4
 context_4_0_5
@@ -410,7 +410,7 @@ fn region_1
 context_4_1_1
 context_4_1_2
-    old_line_4_1
+    new_line_4_1
 context_4_1_4
 context_4_1_5
@@ -420,7 +420,7 @@ fn region_2
 context_4_2_1
 context_4_2_2
-    old_line_4_2
+    new_line_4_2
 context_4_2_4
 context_4_2_5
@@ -430,7 +430,7 @@ fn region_3
 context_4_3_1
 context_4_3_2
-    old_line_4_3
+    new_line_4_3
 context_4_3_4
 context_4_3_5
@@ -440,7 +440,7 @@ fn region_4
 context_4_4_1
 context_4_4_2
-    old_line_4_4
+    new_line_4_4
 context_4_4_4
 context_4_4_5
@@ -450,7 +450,7 @@ fn region_5
 context_4_5_1
 context_4_5_2
-    old_line_4_5
+    new_line_4_5
 context_4_5_4
 context_4_5_5
diff --git a/src/file5.rs b/src/file5.rs
--- a/src/file5.rs
+++ b/src/file5.rs
@@ -500,7 +500,7 @@ fn region_0
 context_5_0_1
 context_5_0_2
-    old_line_5_0
+    new_line_5_0
 context_5_0_4
 context_5_0_5
@@ -510,7 +510,7 @@ fn region_1
 context_5_1_1
 context_5_1_2
-    old_line_5_1
+    new_line_5_1
 context_5_1_4
 context_5_1_5
@@ -520,7 +520,7 @@ fn region_2
 context_5_2_1
 context_5_2_2
-    old_line_5_2
+    new_line_5_2
 context_5_2_4
 context_5_2_5
@@ -530,7 +530,7 @@ fn region_3
 context_5_3_1
 context_5_3_2
-    old_line_5_3
+    new_line_5_3
 context_5_3_4
 context_5_3_5
@@ -540,7 +540,7 @@ fn region_4
 context_5_4_1
 context_5_4_2
-    old_line_5_4
+    new_line_5_4
 context_5_4_4
 context_5_4_5
@@ -550,7 +550,7 @@ fn region_5
 context_5_5_1
 context_5_5_2
-    old_line_5_5
+    new_line_5_5
 context_5_5_4
 context_5_5_5
diff --git a/src/file6.rs b/src/file6.rs
--- a/src/file6.rs
+++ b/src/file6.rs
@@ -600,7 +600,7 @@ fn region_0
 context_6_0_1
 context_6_0_2
-    old_line_6_0
+    new_line_6_0
 context_6_0_4
 context_6_0_5
@@ -610,7 +610,7 @@ fn region_1
 context_6_1_1
 context_6_1_2
-    old_line_6_1
+    new_line_6_1
 context_6_1_4
 context_6_1_5
@@ -620,7 +620,7 @@ fn region_2
 context_6_2_1
 context_6_2_2
-    old_line_6_2
+    new_line_6_2
 context_6_2_4
 context_6_2_5
@@ -630,7 +630,7 @@ fn region_3
 context_6_3_1
 context_6_3_2
-    old_line_6_3
+    new_line_6_3
 context_6_3_4
 context_6_3_5
@@ -640,7 +640,7 @@ fn region_4
 context_6_4_1
 context_6_4_2
-    old_line_6_4
+    new_line_6_4
 context_6_4_4
 context_6_4_5
@@ -650,7 +650,7 @@ fn region_5
 context_6_5_1
 context_6_5_2
-    old_line_6_5
+    new_line_6_5
 context_6_5_4
 context_6_5_5
diff --git a/src/file7.rs b/src/file7.rs
--- a/src/file7.rs
+++ b/src/file7.rs
@@ -700,7 +700,7 @@ fn region_0
 context_7_0_1
 context_7_0_2
-    old_line_7_0
+    new_line_7_0
 context_7_0_4
 context_7_0_5
@@ -710,7 +710,7 @@ fn region_1
 context_7_1_1
 context_7_1_2
-    old_line_7_1
+    new_line_7_1
 context_7_1_4
 context_7_1_5
@@ -720,7 +720,7 @@ fn region_2
 context_7_2_1
 context_7_2_2
-    old_line_7_2
+    new_line_7_2
 context_7_2_4
 context_7_2_5
@@ -730,7 +730,7 @@ fn region_3
 context_7_3_1
 context_7_3_2
-    old_line_7_3
+    new_line_7_3
 context_7_3_4
 context_7_3_5
@@ -740,7 +740,7 @@ fn region_4
 context_7_4_1
 context_7_4_2
-    old_line_7_4
+    new_line_7_4
 context_7_4_4
 context_7_4_5
@@ -750,7 +750,7 @@ fn region_5
 context_7_5_1
 context_7_5_2
-    old_line_7_5
+    new_line_7_5
 context_7_5_4
 context_7_5_5
[8 files changed, +48 -48 lines]
[456 lines compressed to 361. Retrieve full diff: <<ccr:66144f72428c5a93b4683d7f>>]<<ccr:66144f72428c5a93b4683d7f>>
```

## 运行结果

| 指标 | 结果 |
|---|---:|
| 原文字节数 | 8201 |
| 压缩后字节数 | 6910 |
| 压缩后占比 | 84.3% |
| 节省 token（估算） | 397 |
| 检查 block | 1 |
| 压缩 block | 1 |
| 回退 block | 0 |
| 冻结消息 | 1 |
| CCR 写入 | 1 |

- CCR 恢复：PASS（`66144f72428c5a93b4683d7f`）
- 场景断言：PASS
