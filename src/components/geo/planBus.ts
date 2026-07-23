/**
 * 选题「先规划、再定夺」总线：
 * 门户选题池点「生成→投递」不再立刻排产，而是把一个规划请求投到这里 →
 * 常驻的全局对话坞（GlobalChatDock）接住它，在对话框里流式生成一份撰写规划
 * （选题角度 / 核心要点 / 结构大纲），底下摆「开始」「否决」两颗按钮：
 *   - 开始：调用 onApprove() 真正入队并启动全链路 job；
 *   - 否决：丢弃，选题留在池子里，什么都不发生。
 * 这样人先看清「打算怎么写」，再决定要不要真跑，记录都落在对话框里。
 */
import { ref } from "vue";
import type { MediaPlatform } from "../../tauri";

export interface PlanRequest {
  /** 每次请求唯一，避免对话坞重复接同一条。 */
  id: string;
  /** 该规划该落到哪条泳道的对话框（如 `portal:wechat`）。 */
  laneKey: string;
  platform: MediaPlatform;
  platformName: string;
  title: string;
  angle?: string;
  keywords?: string[];
  /** 用户点「开始」后真正执行的排产启动，返回启动的 job id 供对话框给个「看流程」入口。 */
  onApprove: () => Promise<{ jobId: string } | void>;
}

export const planRequest = ref<PlanRequest | null>(null);

let seq = 0;
export function requestPlan(req: Omit<PlanRequest, "id">) {
  seq += 1;
  planRequest.value = { ...req, id: `plan-${seq}` };
}

/** 清掉当前规划请求（不传 id 则无条件清；传 id 则仅当匹配时清，避免误清后来的请求）。 */
export function clearPlan(id?: string) {
  if (!id || planRequest.value?.id === id) planRequest.value = null;
}
