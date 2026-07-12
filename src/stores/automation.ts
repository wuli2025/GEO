import { defineStore } from "pinia";
import { ref } from "vue";
import { convApi, type PermissionMode } from "../tauri";
import { useAppStore } from "./app";
import { useChatStore } from "./chat";

/**
 * 自动化（Automation）板块 —— 板块⑨
 * ──────────────────────────────────────────────────────────────
 * 一个「自动化流程」= 一段编排好的提示词 + 运行配置（在哪个项目跑、什么时候跑、
 * 循环几次、是否深度检测）。运行时在所选项目下新建一个对话，把提示词作为一轮
 * 发给本机 Claude Code（复用 chat 管线，本地轻量化执行），流式结果就是这条对话。
 *
 * 设计参考 WorkBuddy / Codex 的「routine / scheduled task」与阿里悟空的「自动工作流」：
 * 都把「可复用的指令模板 + 触发时机 + 执行上下文」三者解耦。这里走最轻的本地实现：
 * 不引入独立编排引擎，靠一段强 agentic 提示词驱动 Claude 自己完成「搜索→撰写→评价→存草稿」。
 * 不直接对外发布（公众号/小红书），只把成品落到项目草稿箱，由用户自行发送。
 */

export type ScheduleKind = "manual" | "daily" | "interval";
export interface Schedule {
  kind: ScheduleKind;
  /** daily: "HH:MM" */
  time?: string;
  /** interval: 每多少小时跑一次 */
  everyHours?: number;
}

export type ExecEnv = "local" | "sandbox";

export interface AutomationFlow {
  id: string;
  name: string;
  /** lucide 图标名（Automation.vue 里映射成组件） */
  icon: string;
  color: string;
  description: string;
  /** 编排好的提示词正文（创建/编辑对话框里的大文本框） */
  prompt: string;
  /** 在哪个项目里运行；null = 运行时取当前项目 */
  projectId: string | null;
  execEnv: ExecEnv;
  schedule: Schedule;
  /** 循环几次（≥1）；>1 时让流程自我迭代改进 */
  loopCount: number;
  /** 是否深度检测（联网深度搜索 deep-research） */
  deepResearch: boolean;
  /** 运行时强制激活的技能 id（如「微信每日待办」流程要带上 wechat-tasks）；与 deep-research 叠加 */
  skillIds?: string[];
  /** 内置流程（不可删，可改一份副本） */
  builtin?: boolean;
  createdAt: number;
  updatedAt: number;
  lastRunAt?: number;
  /** 上次运行生成的对话 id（用于「缩小版对话框」回看进度） */
  lastConvId?: string;
}

export interface FlowDraft {
  id?: string;
  name: string;
  icon: string;
  color: string;
  description: string;
  prompt: string;
  projectId: string | null;
  execEnv: ExecEnv;
  schedule: Schedule;
  loopCount: number;
  deepResearch: boolean;
}

const STORAGE_KEY = "polaris:automation-flows:v1";
// 一次性清空旧数据(含历史内置流程)的标记；见 load()
const PURGE_KEY = "polaris:automation-flows-purged:v1";

export const FLOW_COLORS = [
  "#2c4661", // 墨蓝
  "#c0392b", // 朱（小红书）
  "#3f6b5a", // 松绿（公众号）
  "#a78c4f", // 金
  "#6b4f7a", // 紫（B站）
];

function uid(): string {
  return Date.now().toString(36) + Math.random().toString(36).slice(2, 8);
}

export const useAutomationStore = defineStore("automation", () => {
  const flows = ref<AutomationFlow[]>([]);

  // 创建 / 编辑对话框
  const editorOpen = ref(false);
  const editorTarget = ref<AutomationFlow | null>(null); // null = 新建

  // 「缩小版对话框」：当前在面板里查看运行进度的对话 id
  const activeConvId = ref<string | null>(null);

  function load() {
    // 内置流程已全部下线：自动化从空列表起步,流程一律由用户自己新建。
    // PURGE 键做一次性清理:把历史版本种进 localStorage 的内置流一并抹掉(只跑一次,
    // 此后用户自建的流程照常持久化)。
    if (!localStorage.getItem(PURGE_KEY)) {
      localStorage.removeItem(STORAGE_KEY);
      localStorage.setItem(PURGE_KEY, "1");
    }
    try {
      const raw = localStorage.getItem(STORAGE_KEY);
      if (raw) flows.value = JSON.parse(raw) as AutomationFlow[];
    } catch {
      /* storage 坏了就当空列表 */
    }
  }

  // 200ms debounce:save/remove/seed 连发时合并成一次序列化
  let persistTimer: ReturnType<typeof setTimeout> | undefined;
  function persist() {
    clearTimeout(persistTimer);
    persistTimer = setTimeout(() => {
      try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify(flows.value));
      } catch {
        /* storage 不可用 */
      }
    }, 200);
  }

  function openCreate() {
    editorTarget.value = null;
    editorOpen.value = true;
  }
  function openEdit(f: AutomationFlow) {
    editorTarget.value = f;
    editorOpen.value = true;
  }
  function closeEditor() {
    editorOpen.value = false;
    editorTarget.value = null;
  }

  function saveFlow(draft: FlowDraft): AutomationFlow {
    const now = Date.now();
    if (draft.id) {
      const i = flows.value.findIndex((f) => f.id === draft.id);
      if (i >= 0) {
        flows.value[i] = {
          ...flows.value[i],
          name: draft.name.trim() || flows.value[i].name,
          icon: draft.icon,
          color: draft.color,
          description: draft.description.trim(),
          prompt: draft.prompt,
          projectId: draft.projectId,
          execEnv: draft.execEnv,
          schedule: draft.schedule,
          loopCount: Math.max(1, draft.loopCount || 1),
          deepResearch: draft.deepResearch,
          updatedAt: now,
        };
        flows.value = [...flows.value];
        persist();
        return flows.value[i];
      }
    }
    const f: AutomationFlow = {
      id: uid(),
      name: draft.name.trim() || "未命名自动化",
      icon: draft.icon || "sparkles",
      color: draft.color,
      description: draft.description.trim(),
      prompt: draft.prompt,
      projectId: draft.projectId,
      execEnv: draft.execEnv,
      schedule: draft.schedule,
      loopCount: Math.max(1, draft.loopCount || 1),
      deepResearch: draft.deepResearch,
      createdAt: now,
      updatedAt: now,
    };
    flows.value = [f, ...flows.value];
    persist();
    return f;
  }

  function removeFlow(id: string) {
    flows.value = flows.value.filter((f) => f.id !== id);
    persist();
  }

  /** 组装最终发给 Claude 的提示词（叠加循环 / 深度检测的框架说明） */
  function composePrompt(f: AutomationFlow): string {
    let p = f.prompt.trim();
    if (f.loopCount > 1) {
      p += `\n\n## 迭代要求\n请把上述流程独立执行并自我迭代共 ${f.loopCount} 轮：每轮在上一轮成品基础上，针对评审发现的最大问题做实质性改进，最终只把**最好的一版**留在草稿箱（其余轮次仅说明改了什么）。`;
    }
    if (f.deepResearch) {
      p += `\n\n（已开启「深度检测」：请尽量多源联网检索、交叉验证，区分事实与观点，并标注来源与时间。）`;
    }
    return p;
  }

  /** 运行一个流程：在所选项目下新建对话，把提示词作为一轮发给本机 Claude（复用 chat 管线） */
  async function runFlow(f: AutomationFlow): Promise<string | null> {
    const app = useAppStore();
    const chat = useChatStore();
    const projectId = f.projectId || app.currentProjectId;
    if (!projectId) return null;

    const conv = await convApi.createConversation(projectId);
    // 让侧栏 / 项目对话列表也能看到这条运行记录
    await app.refreshConversations(projectId).catch(() => {});

    const permissionMode: PermissionMode = "auto_current";
    const skillIds = [
      ...(f.skillIds ?? []),
      ...(f.deepResearch ? ["deep-research"] : []),
    ];
    const prompt = composePrompt(f);
    const display = `自动化「${f.name}」运行中…`;

    await chat.send(conv.id, prompt, display, undefined, {
      permissionMode,
      skillIds,
    });

    // 标记运行态
    const i = flows.value.findIndex((x) => x.id === f.id);
    if (i >= 0) {
      flows.value[i] = { ...flows.value[i], lastRunAt: Date.now(), lastConvId: conv.id };
      flows.value = [...flows.value];
      persist();
    }
    activeConvId.value = conv.id;
    return conv.id;
  }

  // ───────────── 轻量本地调度器：app 开着时按 schedule 触发 ─────────────
  // 每分钟检查一次；daily=到点且当天未跑过则跑；interval=距上次 ≥ everyHours 小时则跑。
  let timer: number | undefined;

  function tick() {
    const now = new Date();
    for (const f of flows.value) {
      const s = f.schedule;
      if (!s || s.kind === "manual") continue;
      const chat = useChatStore();
      // 上一轮还在跑就跳过，避免叠加
      if (f.lastConvId && chat.isSending(f.lastConvId)) continue;

      if (s.kind === "daily" && s.time) {
        const [hh, mm] = s.time.split(":").map((x) => parseInt(x, 10));
        if (Number.isNaN(hh) || Number.isNaN(mm)) continue;
        // 今天的计划时刻。用「时间戳窗口」而非分钟精确相等: 到点或之后(哪怕休眠/定时器
        // 节流错过了那一分钟)且本次计划时刻后还没跑过 → 补跑。lastRunAt 保证当天只跑一次。
        const scheduled = new Date(
          now.getFullYear(), now.getMonth(), now.getDate(), hh, mm, 0, 0
        ).getTime();
        if (now.getTime() >= scheduled && (f.lastRunAt ?? 0) < scheduled) {
          void runFlow(f);
        }
      } else if (s.kind === "interval" && s.everyHours && s.everyHours > 0) {
        const due = (f.lastRunAt ?? 0) + s.everyHours * 3600_000;
        if (Date.now() >= due) void runFlow(f);
      }
    }
  }

  function startScheduler() {
    if (timer != null) return;
    timer = window.setInterval(tick, 60_000);
  }
  function stopScheduler() {
    if (timer != null) {
      clearInterval(timer);
      timer = undefined;
    }
  }

  load();

  return {
    flows,
    editorOpen,
    editorTarget,
    activeConvId,
    openCreate,
    openEdit,
    closeEditor,
    saveFlow,
    removeFlow,
    composePrompt,
    runFlow,
    startScheduler,
    stopScheduler,
  };
});
