**平台：知乎 · 投递流程**

**登录态**：profile `~/PolarisGEO/browser-profiles/zhihu`，未登录交账号管家扫码（zhihu.com/signin）。

**投递范式**：走 draft_uploader.py 的 zhihu 适配器——标题填 `.WriteIndex-titleInput textarea`，正文填 `.DraftEditor-root`（知乎自动存草稿）。文章走专栏 zhuanlan.zhihu.com/write；若为「回答」则定位对应问题的回答编辑器。

**题图（封面）是硬性产物**：`--images` 第一张即题图，走 `cover.direct_input` 专用流程——**先点「添加封面」label（`reveal_click`）激活封面组件，React 才认后续 `set_input_files`**（喂 `input.UploadPicture-input` 真实文件走知乎原生上传）。上传后回读 `img[alt='封面图']` 确认落地。**绝不能靠合成 dataURL 粘贴塞正文首图当封面——知乎 Draft.js 不认，那样等于没封面。** 若 `cover_set ok=false`（未回读到预览），重试一次或明确报告"题图未确认，请到窗口手动拖入"，不得默默略过。

**流程步骤**：打开 zhuanlan.zhihu.com/write → 填标题 → 富文本正文粘贴 → **设题图（点「添加封面」→喂图→回读 `img[alt='封面图']` 确认）** → 知乎自动存草稿（URL 跳 `/p/{id}/edit` 即成功，回执文案常抓不到属正常） → 返回 `draft_uploaded` → 收尾报告里单独讲清题图落位状态。

**平台红线（铁律）**：只存草稿，绝不点「发布」。引用需注明出处；不搬运。

**失败降级**：适配失败或未登录 → 打开 write 页 + 正文进剪贴板 + 提示手动粘贴并自行发布，返回 `manual_assist`。
