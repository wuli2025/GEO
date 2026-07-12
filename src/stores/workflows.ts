import { defineStore } from "pinia";
import { ref } from "vue";

/**
 * 工作流包（Workflow Pack）
 * ──────────────────────────────────────────────────────────────
 * 一个「结构化提示词」：由若干有序「环节(step)」编排而成。
 * 每个环节是一段带标题的提示词正文，调整环节顺序 / 增删即可重新编排，
 * 适合那些需要不断改变编排方式的任务。点「使用」会把所有环节拼装成
 * 一段完整提示词，填入对话输入框。本地持久化（localStorage）。
 */
export interface WorkflowStep {
  id: string;
  label: string; // 环节标题，如「角色」「任务」「约束」「输出格式」
  content: string; // 该环节的提示词正文
}

export interface WorkflowPack {
  id: string;
  name: string;
  description: string;
  color: string; // 强调色（取自 PACK_COLORS）
  steps: WorkflowStep[];
  createdAt: number;
  updatedAt: number;
}

/** 工作流编辑器提交的数据（id 为空 = 新建） */
export interface WorkflowDraft {
  id?: string;
  name: string;
  description: string;
  color: string;
  steps: WorkflowStep[];
}

const STORAGE_KEY = "polaris:workflow-packs:v1";
// 一次性清空旧数据(含历史内置示例包)的标记；见 load()
const PURGE_KEY = "polaris:workflow-packs-purged:v1";
// v2 增补的「写作 / 技术选型三件套」：对老用户也补一次，删除后不回种
// v3 增补「网页演示视频成片 / Claude Code Harness 工程实践」两套（源自 ConardLi 教程）

/** 墨蓝水墨主题取色 —— 每个包一抹强调色 */
export const PACK_COLORS = [
  "#2c4661", // 墨蓝
  "#a78c4f", // 金
  "#c0392b", // 朱
  "#3f6b5a", // 松绿
  "#6b4f7a", // 紫
  "#9c6b3f", // 赭石
];

function uid(): string {
  return Date.now().toString(36) + Math.random().toString(36).slice(2, 8);
}

export function newStep(label = "", content = ""): WorkflowStep {
  return { id: uid(), label, content };
}

/** 把一个工作流包的环节拼装成最终提示词 */
export function assemblePack(p: { steps: WorkflowStep[] }): string {
  return p.steps
    .map((s) => {
      const body = s.content.trim();
      if (!body) return "";
      const label = s.label.trim();
      return label ? `【${label}】\n${body}` : body;
    })
    .filter(Boolean)
    .join("\n\n");
}

export const useWorkflowsStore = defineStore("workflows", () => {
  const packs = ref<WorkflowPack[]>([]);

  // 编辑器（新建 / 修改共用一个模态）
  const editorOpen = ref(false);
  const editorTarget = ref<WorkflowPack | null>(null); // null = 新建

  // 「使用」→ 把拼装文本送进对话输入框；带 nonce 以便重复使用同一包也能触发
  const insertRequest = ref<{ text: string; n: number } | null>(null);
  let insertSeq = 0;

  function load() {
    // 内置示例包已全部下线：工作流包从空列表起步,一律由用户自己新建。
    // PURGE 键做一次性清理:把历史版本种进 localStorage 的示例包一并抹掉(只跑一次)。
    if (!localStorage.getItem(PURGE_KEY)) {
      localStorage.removeItem(STORAGE_KEY);
      localStorage.setItem(PURGE_KEY, "1");
    }
    try {
      const raw = localStorage.getItem(STORAGE_KEY);
      if (raw) packs.value = JSON.parse(raw) as WorkflowPack[];
    } catch {
      /* storage 坏了就当空列表 */
    }
  }

  // 200ms debounce:连续保存/删除合并成一次序列化
  let persistTimer: ReturnType<typeof setTimeout> | undefined;
  function persist() {
    clearTimeout(persistTimer);
    persistTimer = setTimeout(() => {
      try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify(packs.value));
      } catch {
        /* storage 不可用 */
      }
    }, 200);
  }

  function openCreate() {
    editorTarget.value = null;
    editorOpen.value = true;
  }
  function openEdit(p: WorkflowPack) {
    editorTarget.value = p;
    editorOpen.value = true;
  }
  function closeEditor() {
    editorOpen.value = false;
    editorTarget.value = null;
  }

  /** 新建或更新一个包 */
  function savePack(draft: WorkflowDraft) {
    const now = Date.now();
    const steps = draft.steps.filter(
      (s) => s.label.trim() || s.content.trim()
    );
    if (draft.id) {
      const i = packs.value.findIndex((p) => p.id === draft.id);
      if (i >= 0) {
        packs.value[i] = {
          ...packs.value[i],
          name: draft.name.trim(),
          description: draft.description.trim(),
          color: draft.color,
          steps,
          updatedAt: now,
        };
        packs.value = [...packs.value];
      }
    } else {
      packs.value = [
        {
          id: uid(),
          name: draft.name.trim(),
          description: draft.description.trim(),
          color: draft.color,
          steps,
          createdAt: now,
          updatedAt: now,
        },
        ...packs.value,
      ];
    }
    persist();
  }

  function removePack(id: string) {
    packs.value = packs.value.filter((p) => p.id !== id);
    persist();
  }

  /** 点「使用」：拼装并请求填入对话框 */
  function usePack(p: WorkflowPack) {
    insertRequest.value = { text: assemblePack(p), n: ++insertSeq };
  }

  /** 直接请求把一段文字填入对话框(引导向导收尾推荐工作流用) */
  function insertText(text: string) {
    insertRequest.value = { text, n: ++insertSeq };
  }

  function clearInsert() {
    insertRequest.value = null;
  }

  load();

  return {
    packs,
    editorOpen,
    editorTarget,
    insertRequest,
    openCreate,
    openEdit,
    closeEditor,
    savePack,
    removePack,
    usePack,
    insertText,
    clearInsert,
  };
});
