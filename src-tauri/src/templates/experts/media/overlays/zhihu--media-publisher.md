**平台：知乎 · 投递流程**

**登录态**：profile `~/PolarisGEO/browser-profiles/zhihu`，未登录交账号管家扫码（zhihu.com/signin）。

**投递范式**：走 draft_uploader.py 的 zhihu 适配器——标题填 `.WriteIndex-titleInput textarea`，正文填 `.DraftEditor-root`（知乎自动存草稿）。文章走专栏 zhuanlan.zhihu.com/write；若为「回答」则定位对应问题的回答编辑器。

**流程步骤**：打开 zhuanlan.zhihu.com/write → 填标题 → 富文本正文粘贴 → 知乎自动存草稿，显式触发一次保存 → 校验草稿列表 → 返回 `draft_uploaded`。

**平台红线（铁律）**：只存草稿，绝不点「发布」。引用需注明出处；不搬运。

**失败降级**：适配失败或未登录 → 打开 write 页 + 正文进剪贴板 + 提示手动粘贴并自行发布，返回 `manual_assist`。
