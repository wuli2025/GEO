// 仅用于本地视觉预览：给 mediaJob 塞入与线上一致形态的假数据，
// 挂载 JobDetailDrawer 看新版玻璃质感。不参与打包（临时文件）。
import { createApp, h } from "vue";
import { createPinia } from "pinia";
import { mediaJob, type MediaJob } from "./src/tauri";
import JobDetailDrawer from "./src/components/geo/JobDetailDrawer.vue";
import "./src/style.css";

const now = Math.floor(Date.parse("2026-07-17T19:07:11") / 1000);

const fakeJob: MediaJob = {
  id: "test-attr-merge",
  queueId: "q-demo",
  platform: "gzh",
  title: "t",
  topic: "",
  stages: ["generate"],
  status: "failed",
  stage: "generate",
  error: "应用重启中断（历史快照）",
  articlePath: undefined,
  logPath: "/data/polaris/logs/media/test-attr-merge.log",
  createdAt: now,
  updatedAt: now,
  steps: [
    {
      key: "generate",
      label: "生成",
      status: "ok",
      detail: "已落盘",
      at: now,
      startedAt: now,
      durationMs: 0,
      expertId: "media-writer",
      expertName: "主笔",
      skillId: "",
      skillScript: "",
      prompt: "系统设定全文……（此处为演示占位提示词，展开可见更多内容。）\n你是资深公众号主笔，请根据选题方向撰写一篇结构完整、语气自然的正文。",
      promptVersionId: "",
      modelHint: "",
    },
  ],
};

// 覆盖真实 tauri 调用（对象方法可变）
mediaJob.status = async () => JSON.parse(JSON.stringify(fakeJob));
mediaJob.log = async () => "[19:07:11] py> 生成开始\n[19:07:11] 已加载专家:主笔\n[19:07:12] 应用重启中断，本步快照回放结束";
mediaJob.article = async () => "（演示：无正文产物）";

const app = createApp({
  render: () => h(JobDetailDrawer, { jobId: "test-attr-merge", onClose: () => {} }),
});
app.use(createPinia());
app.mount("#app");
