<script setup lang="ts">
/**
 * GEO 自媒体运营中心 —— 应用唯一界面。
 * 原 Polaris 通用外壳(侧栏/对话/文库/文件中心/图谱/工坊等视图)已整体删除:
 * 启动流程只剩 splash → env(环境网关) → 控制台;更新条/Toast/供应商弹窗保留在壳层。
 */
import { ref, onMounted, onBeforeUnmount } from "vue";
import GeoOpsCenter from "./components/GeoOpsCenter.vue";
import FaultBoundary from "./components/FaultBoundary.vue";
import SplashScreen from "./components/SplashScreen.vue";
import EnvDoctor from "./components/EnvDoctor.vue";
import UpdateBanner from "./components/UpdateBanner.vue";
import ToastHost from "./components/ToastHost.vue";
import AddProviderModal from "./components/AddProviderModal.vue";
import { checkForUpdate } from "./composables/useUpdater";
import { onWsStatus, isTauri, invoke } from "./tauri";
import { useProvidersStore } from "./stores/providers";

const providers = useProvidersStore();

// Docker/Web 模式:WS 断线 → 顶部细条提示(自动重连由 tauri.ts 负责)
const wsDown = ref(false);
let unWsStatus: (() => void) | null = null;

// 启动流程:splash(每次) → env(环境检测,健康则无感放行) → ready
const phase = ref<"splash" | "env" | "ready">("splash");
// 开屏「就绪即放行」信号:外壳挂载完成即置 true → 开屏只在防闪的最短展示时间后即淡出
const splashReady = ref(false);

onMounted(() => {
  splashReady.value = true;
  if (isTauri) {
    // 原生标题栏染成控制台底色(Win11 生效,Win10 静默跳过);取自 geo.css 的 --bg/--ink
    invoke("set_titlebar_color", { caption: "#f5f6fa", text: "#1c2233" }).catch(() => {});
  } else {
    unWsStatus = onWsStatus((ok) => (wsDown.value = !ok));
  }
});
onBeforeUnmount(() => unWsStatus?.());

function onEnvDone() {
  phase.value = "ready";
  // splash → env 全部完成后再检查更新(避免弹窗被盖住)
  checkForUpdate();
}
</script>

<template>
  <div class="shell">
    <!-- 控制台在 splash/env 覆盖层底下即挂载,数据边加载边被遮住;
         故障舱壁兜底:渲染/生命周期抛错只换成可重试卡片,不白屏整窗 -->
    <FaultBoundary>
      <GeoOpsCenter />
    </FaultBoundary>

    <!-- 自动更新提示条(发现新版本时浮出) + 全局 toast(统一通知出口) -->
    <UpdateBanner />
    <ToastHost />

    <!-- 供应商添加/编辑弹窗(API 中心「切换中心」的添加/编辑入口) -->
    <AddProviderModal v-if="providers.showAddModal" />

    <!-- Docker/Web 模式断线提示条 -->
    <div v-if="wsDown" class="ws-down">连接已断开,正在自动重连…</div>

    <!-- 启动流程覆盖层:splash → env -->
    <Transition name="splash-fade">
      <SplashScreen v-if="phase === 'splash'" :ready="splashReady" @done="phase = 'env'" />
    </Transition>
    <Transition name="onboard-fade">
      <EnvDoctor v-if="phase === 'env'" gate @done="onEnvDone" />
    </Transition>
  </div>
</template>

<style scoped>
.shell {
  height: 100vh;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}
/* Docker/Web 模式 WS 断线提示条 */
.ws-down {
  position: fixed;
  top: 0;
  left: 50%;
  transform: translateX(-50%);
  z-index: 9998;
  padding: 4px 16px;
  border-radius: 0 0 9px 9px;
  background: var(--vermilion);
  color: #fff;
  font-size: 12px;
  letter-spacing: 0.5px;
  box-shadow: var(--shadow-lg);
}
</style>

<!-- 非 scoped:Transition 类名需作用在子组件根元素上 -->
<style>
.splash-fade-leave-active {
  transition: opacity 0.8s ease;
}
.splash-fade-leave-to {
  opacity: 0;
}
.onboard-fade-enter-active {
  transition: opacity 0.4s ease;
}
.onboard-fade-leave-active {
  transition: opacity 0.45s ease;
}
.onboard-fade-enter-from,
.onboard-fade-leave-to {
  opacity: 0;
}
</style>
