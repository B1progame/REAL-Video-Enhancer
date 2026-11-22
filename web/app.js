const navButtons = document.querySelectorAll('.nav-btn');
const pages = {
  home: document.getElementById('page-home'),
  process: document.getElementById('page-process'),
  download: document.getElementById('page-download'),
  runtime: document.getElementById('page-runtime'),
  python: document.getElementById('page-python'),
  settings: document.getElementById('page-settings'),
};

function activatePage(name) {
  if (!name || !pages[name]) return;
  if (name === 'process' && processNavigationButton?.disabled) return;
  navButtons.forEach((btn) => btn.classList.toggle('active', btn.dataset.page === name));
  Object.entries(pages).forEach(([key, el]) => el.classList.toggle('active', key === name));
}

const processNavigationButton = document.querySelector('.nav-btn[data-page="process"]');
const firstRunSetupDialog = document.getElementById('firstRunSetupDialog');
const firstRunInstallBtn = document.getElementById('firstRunInstallBtn');
const firstRunAnimeScripterBtn = document.getElementById('firstRunAnimeScripterBtn');
const firstRunLaterBtn = document.getElementById('firstRunLaterBtn');
const processStartButton = document.getElementById('startRender');
const homeStartProcessButton = document.getElementById('homeStartProcess');
setProcessAvailability(false);

function setProcessAvailability(available) {
  const enabled = Boolean(available);
  if (processNavigationButton) {
    processNavigationButton.disabled = !enabled;
    processNavigationButton.title = enabled ? 'Process' : 'Install a processing backend before rendering';
    processNavigationButton.setAttribute('aria-disabled', String(!enabled));
  }
  if (processStartButton) processStartButton.disabled = !enabled;
  if (homeStartProcessButton) homeStartProcessButton.disabled = !enabled;
}

function showFirstRunSetupPrompt() {
  firstRunSetupDialog?.classList.remove('anime-only-setup');
  const title = document.getElementById('firstRunSetupTitle');
  const description = document.getElementById('firstRunSetupDescription');
  const includes = document.getElementById('firstRunIncludes');
  if (title) title.textContent = 'Set up video processing?';
  if (description) description.textContent = 'No compatible processing backend is ready yet. Install the recommended setup for this PC, or choose a backend yourself later.';
  if (includes) includes.innerHTML = '<li>Managed Python runtime and RVE libraries</li><li>Recommended PyTorch or NCNN backend for this PC</li><li>FFmpeg for reading and exporting video</li><li>TheAnimeScripter command-line engine</li>';
  if (firstRunInstallBtn) firstRunInstallBtn.hidden = false;
  if (firstRunAnimeScripterBtn) firstRunAnimeScripterBtn.hidden = true;
  if (!firstRunSetupDialog?.open) firstRunSetupDialog?.showModal();
}

function showAnimeScripterSetupPrompt() {
  firstRunSetupDialog?.classList.add('anime-only-setup');
  const title = document.getElementById('firstRunSetupTitle');
  const description = document.getElementById('firstRunSetupDescription');
  const includes = document.getElementById('firstRunIncludes');
  if (title) title.textContent = 'Add the AnimeScripter engine?';
  if (description) description.textContent = 'The RVE engine is ready. Install the official portable AnimeScripter CLI so the AnimeScripter engine and its extra operations work from this launcher.';
  if (includes) includes.innerHTML = '<li>Official portable AnimeScripter command-line engine</li><li>Installed in the RVE application data folder</li><li>Model weights download the first time you use each method</li>';
  if (firstRunInstallBtn) firstRunInstallBtn.hidden = true;
  if (firstRunAnimeScripterBtn) firstRunAnimeScripterBtn.hidden = false;
  if (!firstRunSetupDialog?.open) firstRunSetupDialog?.showModal();
}

firstRunLaterBtn?.addEventListener('click', async () => {
  const animeOnly = firstRunSetupDialog?.classList.contains('anime-only-setup');
  if (animeOnly) {
    try {
      await apiPost('/api/settings', { settings: { animescripter_setup_dismissed: true } });
    } catch (_error) {
    }
  }
  firstRunSetupDialog?.close();
  if (!animeOnly) activatePage('download');
});

firstRunInstallBtn?.addEventListener('click', async () => {
  firstRunSetupDialog?.close();
  try {
    const system = await runRuntimeSetupFromUI();
    if (!system?.available_backends?.length) {
      showToastNotification('Setup needs attention', 'No usable processing backend was detected. Check the Runtime page log.');
    }
  } catch (error) {
    activatePage('runtime');
    appendDownloadLog(`First-run setup failed: ${String(error)}`);
  }
});

firstRunAnimeScripterBtn?.addEventListener('click', async () => {
  firstRunSetupDialog?.close();
  await installAnimeScripterFromUI();
});

navButtons.forEach((btn) => {
  if (!btn.dataset.page) return;
  btn.addEventListener('click', () => activatePage(btn.dataset.page));
});

document.getElementById('homeStartProcess')?.addEventListener('click', () => activatePage('process'));

function setupTabs(wrapperSelector) {
  const wrap = document.querySelector(`[data-tabs="${wrapperSelector}"]`);
  if (!wrap) return;
  const tabButtons = wrap.querySelectorAll('.tab');
  tabButtons.forEach((btn) => {
    btn.addEventListener('click', () => {
      tabButtons.forEach((x) => x.classList.remove('active'));
      btn.classList.add('active');
      const tabId = `tab-${btn.dataset.tab}`;
      const parent = wrap.parentElement;
      parent.querySelectorAll('.tab-panel').forEach((panel) => {
        panel.classList.toggle('active', panel.id === tabId);
      });
    });
  });
}

setupTabs('process-tabs');
setupTabs('settings-tabs');

const queue = document.getElementById('renderQueue');
const outputFiles = document.getElementById('outputFiles');
const engineLog = document.getElementById('engineLog');
const etaText = document.getElementById('etaText');
const fpsText = document.getElementById('fpsText');
const gpuText = document.getElementById('gpuText');
const statusText = document.getElementById('statusText');
const etaToasts = document.getElementById('etaToasts');
const startupScreen = document.getElementById('startupScreen');
const startupStatus = document.getElementById('startupStatus');
const startupDetail = document.getElementById('startupDetail');
const startupProgressFill = document.getElementById('startupProgressFill');
const startupProgressLabel = document.getElementById('startupProgressLabel');
const startupEtaLabel = document.getElementById('startupEtaLabel');
const startupInstallPanel = document.getElementById('startupInstallPanel');
const startupInstallLog = document.getElementById('startupInstallLog');
const startupInstallState = document.getElementById('startupInstallState');
const startupWireframeCanvas = document.getElementById('startupWireframeCanvas');
const startupWireframeSvg = document.getElementById('startupWireframeSvg');
const particles = document.getElementById('particles');
const cursorGlow = document.getElementById('cursorGlow');
const boostVisualsBtn = document.getElementById('boostVisuals');
const quitAppBtn = document.getElementById('quitAppBtn');
const gpuLoad = document.getElementById('gpuLoad');
const throughput = document.getElementById('throughput');
const health = document.getElementById('health');
const gpuMeter = document.getElementById('gpuMeter');
const fpsMeter = document.getElementById('fpsMeter');
const videoRes = document.getElementById('videoRes');
const videoDuration = document.getElementById('videoDuration');
const videoFrames = document.getElementById('videoFrames');
const outputFps = document.getElementById('outputFps');
const inputPathField = document.getElementById('inputPathField');
const outputPathField = document.getElementById('outputPathField');
const renderStartTime = document.getElementById('renderStartTime');
const renderEndTime = document.getElementById('renderEndTime');
const selectInputVideoBtn = document.getElementById('selectInputVideoBtn');
const batchSelectBtn = document.getElementById('batchSelectBtn');
const selectOutputFolderBtn = document.getElementById('selectOutputFolderBtn');
const openOutputFolderBtn = document.getElementById('openOutputFolderBtn');
const backendSelect = document.getElementById('backendSelect');
const processingEngine = document.getElementById('processingEngine');
const animeScripterStatus = document.getElementById('animeScripterStatus');
const installAnimeScripterBtn = document.getElementById('installAnimeScripterBtn');
const animeScripterOptions = document.getElementById('animeScripterOptions');
const interpolateModel = document.getElementById('interpolateModel');
const upscaleModel = document.getElementById('upscaleModel');
const interpolateFactor = document.getElementById('interpolateFactor');
const upscaleScale = document.getElementById('upscaleScale');
const decompressModel = document.getElementById('decompressModel');
const denoiseModel = document.getElementById('denoiseModel');
const deblurModel = document.getElementById('deblurModel');
const livePreviewBox = document.getElementById('livePreviewBox');
const livePreviewImage = document.getElementById('livePreviewVideo');
const livePreviewLabel = document.getElementById('livePreviewLabel');
const processPreviewEnabled = document.getElementById('processPreviewEnabled');
const previewInfoFps = document.getElementById('previewInfoFps');
const previewInfoResolution = document.getElementById('previewInfoResolution');
const previewInfoFrames = document.getElementById('previewInfoFrames');
const previewInfoBitrate = document.getElementById('previewInfoBitrate');
const previewInfoEncoder = document.getElementById('previewInfoEncoder');
const previewInfoContainer = document.getElementById('previewInfoContainer');
const previewInfoColorSpace = document.getElementById('previewInfoColorSpace');
const previewInfoPixelFormat = document.getElementById('previewInfoPixelFormat');
const previewPauseBtn = document.getElementById('previewPauseBtn');
const previewStopBtn = document.getElementById('previewStopBtn');
const previewProgressFill = document.getElementById('previewProgressFill');
const previewProgressText = document.getElementById('previewProgressText');
const previewProgressDetail = document.getElementById('previewProgressDetail');
const previewOverlay = document.getElementById('previewOverlay');
const previewOverlayBg = document.querySelector('.preview-overlay-bg');
const closePreviewOverlay = document.getElementById('closePreviewOverlay');
const livePreviewExpanded = document.getElementById('livePreviewExpanded');
const livePreviewExpandedImage = document.getElementById('livePreviewExpandedVideo');
const livePreviewExpandedLabel = document.getElementById('livePreviewExpandedLabel');

const toggleInterpolate = document.getElementById('toggleInterpolate');
const toggleUpscale = document.getElementById('toggleUpscale');
const toggleDecompress = document.getElementById('toggleDecompress');
const toggleDenoise = document.getElementById('toggleDenoise');
const toggleDeblur = document.getElementById('toggleDeblur');
const toggleSloMo = document.getElementById('toggleSloMo');

const moduleInterpolate = document.getElementById('moduleInterpolate');
const moduleUpscale = document.getElementById('moduleUpscale');
const moduleDecompress = document.getElementById('moduleDecompress');
const moduleDenoise = document.getElementById('moduleDenoise');
const moduleDeblur = document.getElementById('moduleDeblur');
const moduleSloMo = document.getElementById('moduleSloMo');
let selectedIndex = -1;
const videoEncoder = document.getElementById('videoEncoder');
const videoEncoderSpeed = document.getElementById('videoEncoderSpeed');
const videoQualityPreset = document.getElementById('videoQualityPreset');
const videoCrf = document.getElementById('videoCrf');
const videoContainer = document.getElementById('videoContainer');
const videoPixelFormat = document.getElementById('videoPixelFormat');
const audioEncoder = document.getElementById('audioEncoder');
const audioBitrate = document.getElementById('audioBitrate');
const subtitleEncoder = document.getElementById('subtitleEncoder');
const videoTune = document.getElementById('videoTune');
const keyframeInterval = document.getElementById('keyframeInterval');
const customEncoder = document.getElementById('customEncoder');
const encoderPreview = document.getElementById('encoderPreview');
const systemInfoBox = document.getElementById('systemInfoBox');
const refreshBackendsBtn = document.getElementById('refreshBackends');
const installRecommendedBtn = document.getElementById('installRecommended');
const downloadInterpolationModelsBtn = document.getElementById('downloadInterpolationModels');
const downloadUpscaleModelsBtn = document.getElementById('downloadUpscaleModels');
const downloadAllModelsBtn = document.getElementById('downloadAllModels');
const downloadLog = document.getElementById('downloadLog');
const runtimeStatusBadge = document.getElementById('runtimeStatusBadge');
const runtimeStageText = document.getElementById('runtimeStageText');
const runtimeEtaText = document.getElementById('runtimeEtaText');
const runtimeRecommendation = document.getElementById('runtimeRecommendation');
const runtimeRecommendationLabel = document.getElementById('runtimeRecommendationLabel');
const runtimeRecommendationReason = document.getElementById('runtimeRecommendationReason');
const runtimeProgressFill = document.getElementById('runtimeProgressFill');
const runtimeProgressLabel = document.getElementById('runtimeProgressLabel');
const runtimeDetailText = document.getElementById('runtimeDetailText');
const runtimeBackendsText = document.getElementById('runtimeBackendsText');
const runtimeProbeHint = document.getElementById('runtimeProbeHint');
const runtimeBackendReady = document.getElementById('runtimeBackendReady');
const runtimePythonReady = document.getElementById('runtimePythonReady');
const runtimeFfmpegReady = document.getElementById('runtimeFfmpegReady');
const runtimeBackendMeter = document.getElementById('runtimeBackendMeter');
const runtimePythonMeter = document.getElementById('runtimePythonMeter');
const runtimeFfmpegMeter = document.getElementById('runtimeFfmpegMeter');
const runtimeWorkingDir = document.getElementById('runtimeWorkingDir');
const runtimeRootDir = document.getElementById('runtimeRootDir');
const runtimeBackendPath = document.getElementById('runtimeBackendPath');
const runtimePythonPath = document.getElementById('runtimePythonPath');
const runtimeFfmpegPath = document.getElementById('runtimeFfmpegPath');
const runtimeTempPath = document.getElementById('runtimeTempPath');
const runtimeLogState = document.getElementById('runtimeLogState');
const runtimeInstallLog = document.getElementById('runtimeInstallLog');
const runtimeStartBtn = document.getElementById('runtimeStartBtn');
const runtimeRefreshBtn = document.getElementById('runtimeRefreshBtn');
const runtimeOpenFolderBtn = document.getElementById('runtimeOpenFolderBtn');
const runtimeShowDownloadBtn = document.getElementById('runtimeShowDownloadBtn');
const pythonStatusBadge = document.getElementById('pythonStatusBadge');
const pythonPipBadge = document.getElementById('pythonPipBadge');
const pythonRefreshBtn = document.getElementById('pythonRefreshBtn');
const pythonCommandPath = document.getElementById('pythonCommandPath');
const pythonTargetPath = document.getElementById('pythonTargetPath');
const pythonPipLogPath = document.getElementById('pythonPipLogPath');
const pythonInfoOutput = document.getElementById('pythonInfoOutput');
const pythonPipCommand = document.getElementById('pythonPipCommand');
const pythonRunPipBtn = document.getElementById('pythonRunPipBtn');
const pythonClearOutputBtn = document.getElementById('pythonClearOutputBtn');
const pythonPipOutput = document.getElementById('pythonPipOutput');
const pythonCommandState = document.getElementById('pythonCommandState');
const pythonPipVersionBtn = document.getElementById('pythonPipVersionBtn');
const pythonPipListBtn = document.getElementById('pythonPipListBtn');
const pythonPipFreezeBtn = document.getElementById('pythonPipFreezeBtn');
const pythonVerifyBackendBtn = document.getElementById('pythonVerifyBackendBtn');
const hudEta = document.getElementById('hudEta');
const hudGenFps = document.getElementById('hudGenFps');
const hudCpu = document.getElementById('hudCpu');
const hudGpu = document.getElementById('hudGpu');
const hudRam = document.getElementById('hudRam');
const hudVram = document.getElementById('hudVram');
const setUseSameOutputFolder = document.getElementById('setUseSameOutputFolder');
const setOutputFolderLocation = document.getElementById('setOutputFolderLocation');
const setSelectOutputFolderBtn = document.getElementById('setSelectOutputFolderBtn');
const setSceneDetectEnabled = document.getElementById('setSceneDetectEnabled');
const setAutoBorderCropping = document.getElementById('setAutoBorderCropping');
const setSceneDetectMethod = document.getElementById('setSceneDetectMethod');
const setSceneDetectThreshold = document.getElementById('setSceneDetectThreshold');
const setSceneDetectModel = document.getElementById('setSceneDetectModel');
const setVideoContainer = document.getElementById('setVideoContainer');
const setVideoPixelFormat = document.getElementById('setVideoPixelFormat');
const setPrecision = document.getElementById('setPrecision');
const setNcnnGpuId = document.getElementById('setNcnnGpuId');
const setPytorchGpuId = document.getElementById('setPytorchGpuId');
const setTensorRTOptLevel = document.getElementById('setTensorRTOptLevel');
const setAutoHDRMode = document.getElementById('setAutoHDRMode');
const setTensorRTDynamicEngine = document.getElementById('setTensorRTDynamicEngine');
const setUHDMode = document.getElementById('setUHDMode');
const setDynamicScaledFlow = document.getElementById('setDynamicScaledFlow');
const setEnsemble = document.getElementById('setEnsemble');
const setPreviewEnabled = document.getElementById('setPreviewEnabled');
const setDiscordRPC = document.getElementById('setDiscordRPC');
const setCustomInterpolateModel = document.getElementById('setCustomInterpolateModel');
const setCustomUpscaleModel = document.getElementById('setCustomUpscaleModel');
const setBackendManagerTarget = document.getElementById('setBackendManagerTarget');
const backendConfigPytorch = document.getElementById('backendConfigPytorch');
const backendConfigTensorRt = document.getElementById('backendConfigTensorRt');
const setBackendTorchVersion = document.getElementById('setBackendTorchVersion');
const setBackendTorchBackend = document.getElementById('setBackendTorchBackend');
const setBackendTensorRtTorchVersion = document.getElementById('setBackendTensorRtTorchVersion');
const setBackendTensorRtBackend = document.getElementById('setBackendTensorRtBackend');
const setBackendTensorRTOptLevel = document.getElementById('setBackendTensorRTOptLevel');
const setBackendTensorRTDynamicEngine = document.getElementById('setBackendTensorRTDynamicEngine');
const openRveFolderBtn = document.getElementById('openRveFolderBtn');
const resetSettingsBtn = document.getElementById('resetSettingsBtn');
const pickCustomInterpolateModelBtn = document.getElementById('pickCustomInterpolateModel');
const pickCustomUpscaleModelBtn = document.getElementById('pickCustomUpscaleModel');
const importCustomModelsBtn = document.getElementById('importCustomModelsBtn');
const importCustomModelFolderBtn = document.getElementById('importCustomModelFolderBtn');
const customModelFilesInput = document.getElementById('customModelFilesInput');
const customModelFolderInput = document.getElementById('customModelFolderInput');
const customModelSingleInput = document.getElementById('customModelSingleInput');
const refreshCustomModelsBtn = document.getElementById('refreshCustomModelsBtn');
const openCustomModelsFolderBtn = document.getElementById('openCustomModelsFolderBtn');
const customModelsFolderPath = document.getElementById('customModelsFolderPath');
const customModelsCount = document.getElementById('customModelsCount');
const customModelsList = document.getElementById('customModelsList');
const settingsInstalledBackends = document.getElementById('settingsInstalledBackends');
const settingsRecommendedBackend = document.getElementById('settingsRecommendedBackend');
const settingsBackendDiskFree = document.getElementById('settingsBackendDiskFree');
const settingsInstallRecommendedBtn = document.getElementById('settingsInstallRecommendedBtn');
const settingsRefreshBackendsBtn = document.getElementById('settingsRefreshBackendsBtn');
const settingsVerifyBackendBtn = document.getElementById('settingsVerifyBackendBtn');
const settingsInstallNcnnBtn = document.getElementById('settingsInstallNcnnBtn');
const settingsUninstallNcnnBtn = document.getElementById('settingsUninstallNcnnBtn');
const settingsInstallTorchBtn = document.getElementById('settingsInstallTorchBtn');
const settingsUninstallTorchBtn = document.getElementById('settingsUninstallTorchBtn');
const settingsInstallTensorRtBtn = document.getElementById('settingsInstallTensorRtBtn');
const settingsUninstallTensorRtBtn = document.getElementById('settingsUninstallTensorRtBtn');
const settingsInstallDirectMlBtn = document.getElementById('settingsInstallDirectMlBtn');
const settingsUninstallDirectMlBtn = document.getElementById('settingsUninstallDirectMlBtn');
const settingsBackendReason = document.getElementById('settingsBackendReason');
const settingsBackendSupportNote = document.getElementById('settingsBackendSupportNote');
const settingsBackendLog = document.getElementById('settingsBackendLog');
const advBenchmarkMode = document.getElementById('advBenchmarkMode');
const advTiling = document.getElementById('advTiling');
const advMotionBlur = document.getElementById('advMotionBlur');
const advMotionBlurGamma = document.getElementById('advMotionBlurGamma');
const advMotionBlurShutterAngle = document.getElementById('advMotionBlurShutterAngle');
const advMotionBlurStrength = document.getElementById('advMotionBlurStrength');
const advDeduplicate = document.getElementById('advDeduplicate');
const advDeduplicateThreshold = document.getElementById('advDeduplicateThreshold');
const advDynamicScaledFlowMirror = document.getElementById('advDynamicScaledFlowMirror');
const advEnsembleMirror = document.getElementById('advEnsembleMirror');
const advTileSize = document.getElementById('advTileSize');
const advTileOverlap = document.getElementById('advTileOverlap');
const advOverwrite = document.getElementById('advOverwrite');
const advDevice = document.getElementById('advDevice');
const advOutputToMpv = document.getElementById('advOutputToMpv');
const advMergeSubtitles = document.getElementById('advMergeSubtitles');
const advTensorRTOptMirror = document.getElementById('advTensorRTOptMirror');

const generationState = {
  eta: '--',
  fps: null,
  status: 'Ready',
  frame: 0,
  downloadActive: false,
  downloadProgress: 0,
  downloadLabel: '',
  downloadDetail: '',
};

const startupState = {
  startedAt: performance.now(),
  progress: 0,
  loader: null,
  installLogCount: 0,
};

const currentVideoInfo = {
  width: 0,
  height: 0,
  fps: 0,
  frames: 0,
  duration_seconds: 0,
  bitrate: null,
  codec: null,
  container: null,
  color_space: null,
  pixel_format: null,
};

let latestRuntimeMetrics = {
  cpu_percent: null,
  gpu_util_percent: null,
  ram_percent: null,
  ram_used_gb: null,
  ram_total_gb: null,
  vram_used_gb: null,
  vram_total_gb: null,
};
let latestSystemInfo = null;
let latestBackendManagerState = null;
let backendConnectionStatus = 'checking';

let renderStatusPollHandle = null;
let runtimeInstallPollHandle = null;
let runtimeMetricsPollHandle = null;
let runtimeMetricsFetchInFlight = false;
let firstFrameObjectUrl = null;
let previewFrameObjectUrl = null;
let previewFrameFetchInFlight = false;
let lastEtaToastValue = '--';
let lastEtaToastAt = 0;
let liveEtaToast = null;
let liveModelDownloadToast = null;
let lastModelDownloadLabel = '';
const ENGINE_LOG_MAX_LINES = 1800;
let renderedBackendLogCount = 0;

const MODEL_CATALOG = {
  ncnn: {
    interpolate: [
      'RIFE 4.6 (Fastest Model)', 'RIFE 4.7 (Smoothest Model)', 'RIFE 4.15', 'RIFE 4.18 (Realistic)',
      'RIFE 4.22 (Slow, Animation)', 'RIFE 4.22-lite (Latest LITE model)', 'RIFE 4.25',
      'RIFE 4.26 Heavy (Slowest RIFE Model, Animation)', 'RIFE 4.26 (Latest General Model, Recommended)'
    ],
    upscale: [
      'Nomos8k (Realistic) (High Quality Source) (4X) (Fast)',
      'Nomos8k (Realistic) (Medium Quality Source) (4X) (Fast)',
      'Nomos8k (Realistic) (Low Quality Source) (4X) (Fast)',
      'OpenProteus (Realistic) (HD Input) (2X) (Fast)',
      'RealCUGAN Pro (Animation) (2X) (Slow)',
      'RealCUGAN Pro (Animation) (3X) (Slow)',
      'RealisticVideo (4X) (Fast)',
      'JaNai V2 (Animation) (2X) (Fast)',
      'JaNai V3 (Animation) (2X) (Fast)',
      'Spanimation (Animation) (2X) (Fast)',
      'ESRGAN AnimeVideo V3 (Animation) (2X) (Fast)',
      'AniSD (Old Animation) (High Quality Source) (2X) (Fast)',
      'AniSD (Old Animation) (Medium Quality Source) (2X) (Fast)',
      'AniSD (Old Animation) (Low Quality Source) (2X) (Fast)',
      'ESRGAN AnimeVideo V3 (Animation) (3X) (Fast)',
      'ESRGAN AnimeVideo V3 (Animation) (4X) (Fast)',
      'ESRGAN Plus (General Model) (4X) (Slow)',
      'ESRGAN Plus (Animation Model) (4X) (Slow)'
    ],
    decompress: [
      'DeH264 SuperUltraCompact (UltraFast) (High/Medium Quality Source)',
      'DeH264 SPAN (Fast) (Medium/Low Quality Source)',
      'DeH264 RTMoSR (Fast) (High/Medium Quality Source)'
    ],
    denoise: ['DNcnn (Fast)'],
    deblur: ['NAFNet GoPro (Deblur) (Slow)'],
  },
  pytorch: {
    interpolate: [
      'GMFSS (Slow Model, Animation/Realistic)',
      'GMFSS Pro (Slow Model, Animation/Realistic) (Helps with text warping)',
      'GIMM (Slow Model, Realistic/General)',
      'IFRNet (Fast Model, Realistic only)',
      'RIFE 4.6 (Fastest Model)', 'RIFE 4.7 (Smoothest Model)', 'RIFE 4.15', 'RIFE 4.18 (Realistic)',
      'RIFE 4.22 (Slow, Animation)', 'RIFE 4.22-lite (Latest LITE model)', 'RIFE 4.25',
      'RIFE 4.26 Heavy (Slowest RIFE Model, Animation)', 'RIFE 4.26 (Latest General Model, Recommended)'
    ],
    upscale: [
      'Nomos8k (Realistic) (High Quality Source) (4X) (Fast)',
      'Nomos8k (Realistic) (Medium Quality Source) (4X) (Fast)',
      'Nomos8k (Realistic) (Low Quality Source) (4X) (Fast)',
      'OpenProteus (Realistic) (HD Input) (2X) (Fast)',
      'RealisticVideo (4X) (Fast)',
      'JaNai V2 (Animation) (2X) (Fast)',
      'JaNai V3 (Animation) (2X) (Fast)',
      'Sudo Shuffle SPAN (Animation) (2X) (Fast)',
      'SPAN Spanimation V2 (Animation) (2X) (Fast)',
      'SPANPlus Spanimation V3 (Animation) (2X) (Fast)',
      'SPANPlus Dynamic BHI (Realistic) (2X) (Fast)',
      'AniSD (Old Animation) (High Quality Source) (2X) (Fast)',
      'AniSD (Old Animation) (Medium Quality Source) (2X) (Fast)',
      'AniSD (Old Animation) (Low Quality Source) (2X) (Fast)',
      'AnimeSR (Animation) (Low-High Quality Source) (4X) (Slow)'
    ],
    decompress: [
      'DeH264 SuperUltraCompact (UltraFast) (High/Medium Quality Source)',
      'DeH264 RTMoSR (Fast) (High/Medium Quality Source)',
      'DeH264 SPAN (Fast) (Medium/Low Quality Source)',
      'DeH264 PLKSR (Very Slow) (Low Quality Source)'
    ],
    denoise: ['DRUNET (Slow)', 'DNcnn (Fast)', 'SCUNet Color (Slow)'],
    deblur: ['NAFNet GoPro (Deblur) (Fast)'],
  },
  tensorrt: {
    interpolate: [
      'GMFSS (Slow Model, Animation)',
      'GMFSS Pro (Slow Model, Animation) (Helps with text warping)',
      'RIFE 4.6 (Fastest Model)', 'RIFE 4.7 (Smoothest Model)', 'RIFE 4.15', 'RIFE 4.18 (Realistic)',
      'RIFE 4.22 (Slow, Animation)', 'RIFE 4.22-lite (Latest LITE model)', 'RIFE 4.25',
      'RIFE 4.26 Heavy (Slowest RIFE Model, Animation)', 'RIFE 4.26 (Latest General Model, Recommended)'
    ],
    upscale: [
      'Nomos8k (Realistic) (High Quality Source) (4X) (Fast)',
      'Nomos8k (Realistic) (Medium Quality Source) (4X) (Fast)',
      'Nomos8k (Realistic) (Low Quality Source) (4X) (Fast)',
      'OpenProteus (Realistic) (HD Input) (2X) (Fast)',
      'RealisticVideo (4X) (Fast)',
      'JaNai V2 (Animation) (2X) (Fast)',
      'JaNai V3 (Animation) (2X) (Fast)',
      'Spanimation V2 (Animation) (2X) (Fast)',
      'AniSD (Old Animation) (High Quality Source) (2X) (Fast)',
      'AniSD (Old Animation) (Medium Quality Source) (2X) (Fast)',
      'AniSD (Old Animation) (Low Quality Source) (2X) (Fast)'
    ],
    decompress: [
      'DeH264 SuperUltraCompact (UltraFast) (High/Medium Quality Source)',
      'DeH264 RTMoSR (Fast) (High/Medium Quality Source)',
      'DeH264 SPAN (Fast) (Medium/Low Quality Source)'
    ],
    denoise: ['DRUNET (Slow)'],
    deblur: ['NAFNet GoPro (Deblur) (Fast)'],
  },
  directml: {
    interpolate: ['RIFE 4.22 (Recommended Model)'],
    upscale: ['SPAN (Animation) (2X)'],
    decompress: [],
    denoise: [],
    deblur: [],
  },
};

async function apiGet(path) {
  const response = await fetch(path, { method: 'GET' });
  if (!response.ok) throw new Error(`GET ${path} failed with ${response.status}`);
  return response.json();
}

async function apiPost(path, payload = {}) {
  const response = await fetch(path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
  });
  const contentType = response.headers.get('content-type') || '';
  let parsed = null;
  if (contentType.includes('application/json')) {
    try {
      parsed = await response.json();
    } catch (_err) {
      parsed = null;
    }
  } else {
    try {
      parsed = await response.text();
    } catch (_err) {
      parsed = null;
    }
  }
  if (!response.ok) {
    const detail = (parsed && typeof parsed === 'object' && parsed.error)
      ? parsed.error
      : (typeof parsed === 'string' && parsed ? parsed : `POST ${path} failed with ${response.status}`);
    const error = new Error(detail);
    error.code = parsed && typeof parsed === 'object' ? parsed.code : undefined;
    error.outputFile = parsed && typeof parsed === 'object' ? parsed.output_file : undefined;
    throw error;
  }
  return parsed;
}

function sleep(ms) {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}

function formatBootTime(ms) {
  const totalSeconds = Math.max(0, Math.ceil((Number(ms) || 0) / 1000));
  if (totalSeconds < 60) return `${totalSeconds}s`;
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}m ${String(seconds).padStart(2, '0')}s`;
}

function clamp01(value) {
  return Math.max(0, Math.min(1, Number(value) || 0));
}

function lerp(a, b, t) {
  return a + (b - a) * t;
}

function smoothstep(t) {
  const v = clamp01(t);
  return v * v * (3 - (2 * v));
}

function rotateX([x, y, z], angle) {
  const c = Math.cos(angle);
  const s = Math.sin(angle);
  return [x, (y * c) - (z * s), (y * s) + (z * c)];
}

function rotateY([x, y, z], angle) {
  const c = Math.cos(angle);
  const s = Math.sin(angle);
  return [(x * c) + (z * s), y, (-x * s) + (z * c)];
}

function rotateZ([x, y, z], angle) {
  const c = Math.cos(angle);
  const s = Math.sin(angle);
  return [(x * c) - (y * s), (x * s) + (y * c), z];
}

class StartupWireframeLoader {
  constructor(canvas, svg) {
    this.canvas = canvas;
    this.host = canvas?.parentElement || svg?.parentElement || null;
    this.svg = svg;
    this.ctx = canvas ? canvas.getContext('2d', { alpha: true }) : null;
    if (!this.ctx && canvas) {
      this.ctx = canvas.getContext('2d');
    }
    this.running = false;
    this.raf = 0;
    this.startAt = 0;
    this.reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    this.shapeLoopSeconds = 18;
    this.cameraDistance = 6.4;
    this.dpr = 1;
    this.resizeObserver = null;
    this.useSvg = !this.ctx;
    this.canvasVerified = false;
    this.svgLines = [];
    this.edges = [
      [0, 1], [1, 2], [2, 3], [3, 0],
      [4, 5], [5, 6], [6, 7], [7, 4],
      [0, 4], [1, 5], [2, 6], [3, 7],
      [0, 6], [1, 7], [2, 4], [3, 5],
    ];
    this.states = this._buildStates();
    this._boundLoop = (time) => this._loop(time);
    this._syncRenderMode();
    this._ensureSvgLines();
  }

  _buildStates() {
    const cube = [
      [-1, -1, -1], [1, -1, -1], [1, 1, -1], [-1, 1, -1],
      [-1, -1, 1], [1, -1, 1], [1, 1, 1], [-1, 1, 1],
    ];
    const prism = cube.map(([x, y, z]) => [x * 1.45, y * 0.76, z * 1.08]);
    const pinched = cube.map(([x, y, z]) => [x * 0.62, y * 0.62, z * 1.42]);
    const lattice = cube.map(([x, y, z]) => [x * 1.58, y * 1.12, z * 0.14]);
    const twisted = cube.map(([x, y, z]) => {
      if (z < 0) return [x * 1.05, y * 1.05, z];
      const angle = Math.PI / 4;
      const rx = (x * Math.cos(angle)) - (y * Math.sin(angle));
      const ry = (x * Math.sin(angle)) + (y * Math.cos(angle));
      return [rx * 1.1, ry * 1.1, z * 1.02];
    });
    const expanded = cube.map(([x, y, z]) => [
      (x * 1.2) + (y * 0.22),
      (y * 1.2) + (z * 0.22),
      (z * 1.2) + (x * 0.22),
    ]);
    return [cube, prism, pinched, lattice, twisted, expanded];
  }

  start() {
    if ((!this.canvas && !this.svg) || this.running) return;
    this.running = true;
    this.startAt = performance.now();
    this._resize();
    if ('ResizeObserver' in window) {
      this.resizeObserver = new ResizeObserver(() => this._resize());
      this.resizeObserver.observe(this.canvas);
    } else {
      window.addEventListener('resize', () => this._resize());
    }
    // Let layout settle once before first paint so we do not start in a 1x1 canvas.
    this.raf = window.requestAnimationFrame((time) => {
      this._resize();
      this._loop(time);
    });
  }

  stop() {
    this.running = false;
    if (this.raf) {
      window.cancelAnimationFrame(this.raf);
      this.raf = 0;
    }
    if (this.resizeObserver) {
      this.resizeObserver.disconnect();
      this.resizeObserver = null;
    }
  }

  _resize() {
    if (!this.canvas) return;
    let rect = this.canvas.getBoundingClientRect();
    if (rect.width < 8 || rect.height < 8) {
      const hostRect = this.canvas.parentElement?.getBoundingClientRect();
      if (hostRect && hostRect.width > 8 && hostRect.height > 8) {
        rect = hostRect;
      }
    }
    this.dpr = Math.max(1, Math.min(2, window.devicePixelRatio || 1));
    const width = Math.max(1, Math.round(rect.width * this.dpr));
    const height = Math.max(1, Math.round(rect.height * this.dpr));
    if (this.canvas.width !== width || this.canvas.height !== height) {
      this.canvas.width = width;
      this.canvas.height = height;
    }
  }

  _ensureSvgLines() {
    if (!this.svg || this.svgLines.length) return;
    const ns = 'http://www.w3.org/2000/svg';
    for (let i = 0; i < this.edges.length; i += 1) {
      const glow = document.createElementNS(ns, 'line');
      glow.setAttribute('stroke-linecap', 'round');
      glow.setAttribute('stroke-linejoin', 'round');
      this.svg.appendChild(glow);
      const core = document.createElementNS(ns, 'line');
      core.setAttribute('stroke-linecap', 'round');
      core.setAttribute('stroke-linejoin', 'round');
      this.svg.appendChild(core);
      this.svgLines.push({ glow, core });
    }
  }

  _syncRenderMode() {
    if (!this.host) return;
    this.host.classList.toggle('svg-loader-active', !!this.useSvg);
  }

  _sampleShape(phase01) {
    const fallbackCube = [
      [-1, -1, -1], [1, -1, -1], [1, 1, -1], [-1, 1, -1],
      [-1, -1, 1], [1, -1, 1], [1, 1, 1], [-1, 1, 1],
    ];
    const states = Array.isArray(this.states) && this.states.length
      ? this.states.filter((state) => Array.isArray(state) && state.length === fallbackCube.length)
      : [fallbackCube];
    if (!states.length) return fallbackCube;
    const phase = Number.isFinite(phase01) ? phase01 : 0;
    const count = states.length;
    const segment = (phase % 1) * count;
    const indexA = Math.floor(segment) % count;
    const indexB = (indexA + 1) % count;
    const blend = smoothstep(segment - Math.floor(segment));
    const stateA = states[indexA] || fallbackCube;
    const stateB = states[indexB] || stateA;
    return stateA.map(([ax, ay, az], i) => {
      const [bx, by, bz] = stateB[i];
      return [
        lerp(ax, bx, blend),
        lerp(ay, by, blend),
        lerp(az, bz, blend),
      ];
    });
  }

  _projectVertex(vertex, centerX, centerY, scale) {
    const zCamera = vertex[2] + this.cameraDistance;
    const inv = scale / Math.max(0.001, zCamera);
    return {
      x: centerX + (vertex[0] * inv),
      y: centerY + (vertex[1] * inv),
      z: vertex[2],
    };
  }

  _drawHolographicStage(ctx, width, height, elapsed, pulse) {
    const cx = width * 0.5;
    const cy = height * 0.5;
    const radius = Math.min(width, height) * 0.39;
    const breathe = this.reduceMotion ? 1 : pulse;

    const halo = ctx.createRadialGradient(cx, cy, radius * 0.08, cx, cy, radius * 1.55);
    halo.addColorStop(0, 'rgba(74, 232, 220, 0.16)');
    halo.addColorStop(0.42, 'rgba(48, 157, 255, 0.075)');
    halo.addColorStop(1, 'rgba(26, 73, 133, 0)');
    ctx.fillStyle = halo;
    ctx.fillRect(0, 0, width, height);

    ctx.save();
    ctx.translate(cx, cy);
    for (let ring = 0; ring < 3; ring += 1) {
      const ringRadius = radius * (0.83 + (ring * 0.22)) * breathe;
      const tilt = -0.28 + (ring * 0.28);
      ctx.save();
      ctx.rotate(tilt + (this.reduceMotion ? 0 : elapsed * (ring % 2 ? -0.045 : 0.055)));
      ctx.beginPath();
      ctx.ellipse(0, 0, ringRadius, ringRadius * (0.27 + ring * 0.025), 0, 0, Math.PI * 2);
      const orbit = ctx.createLinearGradient(-ringRadius, 0, ringRadius, 0);
      orbit.addColorStop(0, 'rgba(77, 231, 219, 0.03)');
      orbit.addColorStop(0.32, 'rgba(91, 229, 236, 0.38)');
      orbit.addColorStop(0.68, 'rgba(101, 166, 255, 0.27)');
      orbit.addColorStop(1, 'rgba(100, 166, 255, 0.025)');
      ctx.strokeStyle = orbit;
      ctx.lineWidth = ring === 1 ? 1.35 : 0.8;
      ctx.shadowColor = 'rgba(88, 215, 255, 0.5)';
      ctx.shadowBlur = ring === 1 ? 12 : 7;
      ctx.stroke();
      ctx.restore();
    }
    ctx.restore();

    // A thin light sheet travels through the floating model, like a frame scan.
    const scanY = cy + Math.sin(elapsed * (this.reduceMotion ? 0 : 0.72)) * radius * 0.72;
    const scan = ctx.createLinearGradient(0, scanY - 12, 0, scanY + 12);
    scan.addColorStop(0, 'rgba(76, 238, 220, 0)');
    scan.addColorStop(0.5, 'rgba(91, 233, 239, 0.12)');
    scan.addColorStop(1, 'rgba(94, 175, 255, 0)');
    ctx.fillStyle = scan;
    ctx.fillRect(cx - radius * 1.25, scanY - 12, radius * 2.5, 24);

    // Sparse, stable motes make the depth legible without a noisy particle field.
    for (let i = 0; i < 18; i += 1) {
      const seed = (i * 2.3999632297);
      const drift = this.reduceMotion ? 0 : elapsed * (0.1 + ((i % 4) * 0.025));
      const orbitRadius = radius * (0.48 + ((i * 17 % 47) / 100));
      const x = cx + Math.cos(seed + drift) * orbitRadius;
      const y = cy + Math.sin(seed * 1.31 + drift * 0.75) * orbitRadius * 0.7;
      const twinkle = 0.24 + (0.55 * ((Math.sin(elapsed * 1.4 + i) + 1) * 0.5));
      ctx.beginPath();
      ctx.arc(x, y, i % 5 === 0 ? 1.65 : 0.9, 0, Math.PI * 2);
      ctx.fillStyle = `rgba(${i % 3 === 0 ? '100, 255, 222' : '134, 195, 255'}, ${twinkle})`;
      ctx.shadowColor = 'rgba(83, 228, 227, 0.75)';
      ctx.shadowBlur = 8;
      ctx.fill();
    }
    ctx.shadowBlur = 0;
  }

  _drawCore(ctx, raw, centerX, centerY, scale, rotations) {
    const [rotX, rotY, rotZ] = rotations;
    const inner = raw.map((vertex) => {
      let v = vertex.map((axis) => axis * 0.48);
      v = rotateX(v, rotX);
      v = rotateY(v, rotY + Math.PI / 4);
      v = rotateZ(v, rotZ);
      return this._projectVertex(v, centerX, centerY, scale);
    });
    const coreEdges = [
      [0, 1], [1, 2], [2, 3], [3, 0], [4, 5], [5, 6], [6, 7], [7, 4],
      [0, 4], [1, 5], [2, 6], [3, 7],
    ];
    ctx.save();
    ctx.lineWidth = 2.2;
    ctx.strokeStyle = 'rgba(92, 255, 220, 0.9)';
    ctx.shadowColor = 'rgba(73, 241, 217, 0.95)';
    ctx.shadowBlur = 18;
    ctx.beginPath();
    for (const [a, b] of coreEdges) {
      ctx.moveTo(inner[a].x, inner[a].y);
      ctx.lineTo(inner[b].x, inner[b].y);
    }
    ctx.stroke();
    ctx.shadowBlur = 0;
    ctx.fillStyle = '#d9fff8';
    for (const vertex of inner) {
      ctx.beginPath();
      ctx.arc(vertex.x, vertex.y, 2.1, 0, Math.PI * 2);
      ctx.fill();
    }
    const badgeW = 58;
    const badgeH = 34;
    const badgeX = centerX - badgeW / 2;
    const badgeY = centerY - badgeH / 2;
    const badge = ctx.createLinearGradient(badgeX, badgeY, badgeX + badgeW, badgeY + badgeH);
    badge.addColorStop(0, 'rgba(15, 35, 49, 0.96)');
    badge.addColorStop(1, 'rgba(13, 26, 39, 0.96)');
    ctx.fillStyle = badge;
    ctx.strokeStyle = 'rgba(125, 255, 230, 0.8)';
    ctx.lineWidth = 1.2;
    ctx.shadowColor = 'rgba(75, 239, 215, 0.65)';
    ctx.shadowBlur = 18;
    ctx.beginPath();
    ctx.roundRect(badgeX, badgeY, badgeW, badgeH, 10);
    ctx.fill();
    ctx.stroke();
    ctx.shadowBlur = 0;
    ctx.fillStyle = '#d9fff8';
    ctx.font = '700 13px Inter, sans-serif';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText('RVE', centerX, centerY + 0.5);
    ctx.restore();
  }

  _loop(time) {
    if (!this.running || (!this.ctx && !this.svg) || (!this.canvas && !this.svg)) return;
    this._renderFrame(time);
    this.raf = window.requestAnimationFrame(this._boundLoop);
  }

  _renderFrame(time) {
    const width = this.canvas ? this.canvas.width : 320;
    const height = this.canvas ? this.canvas.height : 320;
    const viewW = this.canvas ? width / this.dpr : 320;
    const viewH = this.canvas ? height / this.dpr : 320;
    const centerX = viewW * 0.5;
    const centerY = viewH * 0.5;
    const elapsedSeconds = (time - this.startAt) / 1000;
    const morphPhase = this.reduceMotion ? 0 : ((elapsedSeconds % this.shapeLoopSeconds) / this.shapeLoopSeconds);

    const raw = this._sampleShape(morphPhase);
    const pulse = this.reduceMotion ? (1 + (Math.sin(elapsedSeconds * 0.7) * 0.03)) : 1;
    const rotX = this.reduceMotion ? 0.46 : (Math.sin(elapsedSeconds * 0.5) * 0.42);
    const rotY = elapsedSeconds * (this.reduceMotion ? 0.26 : 0.36);
    const rotZ = this.reduceMotion ? 0 : (Math.cos(elapsedSeconds * 0.4) * 0.22);
    const scale = Math.min(viewW, viewH) * (this.reduceMotion ? 1.9 : 2.02) * pulse;

    const transformed = raw.map((vertex) => {
      let v = vertex;
      v = rotateX(v, rotX);
      v = rotateY(v, rotY);
      v = rotateZ(v, rotZ);
      return this._projectVertex(v, centerX, centerY, scale);
    });

    const segments = this.edges.map(([a, b]) => {
      const va = transformed[a];
      const vb = transformed[b];
      return {
        va,
        vb,
        depth: (va.z + vb.z) * 0.5,
      };
    }).sort((left, right) => left.depth - right.depth);

    if (this.useSvg) {
      this._renderSvg(segments);
      return;
    }

    const ctx = this.ctx;
    if (!ctx || !this.canvas) return;
    ctx.clearRect(0, 0, width, height);
    ctx.save();
    ctx.scale(this.dpr, this.dpr);

    this._drawHolographicStage(ctx, viewW, viewH, elapsedSeconds, pulse);

    ctx.lineCap = 'round';
    ctx.lineJoin = 'round';

    for (const segment of segments) {
      const depthNorm = clamp01((segment.depth + 2) / 4);
      const alpha = 0.22 + (depthNorm * 0.18);
      ctx.strokeStyle = `rgba(75, 213, 235, ${alpha})`;
      ctx.lineWidth = 4.4;
      ctx.shadowColor = 'rgba(76, 203, 255, 0.7)';
      ctx.shadowBlur = 14;
      ctx.beginPath();
      ctx.moveTo(segment.va.x, segment.va.y);
      ctx.lineTo(segment.vb.x, segment.vb.y);
      ctx.stroke();
    }
    ctx.shadowBlur = 0;

    for (const segment of segments) {
      const depthNorm = clamp01((segment.depth + 2) / 4);
      const alpha = 0.6 + (depthNorm * 0.35);
      ctx.strokeStyle = `rgba(218, 250, 255, ${alpha})`;
      ctx.lineWidth = 1.45;
      ctx.beginPath();
      ctx.moveTo(segment.va.x, segment.va.y);
      ctx.lineTo(segment.vb.x, segment.vb.y);
      ctx.stroke();
    }

    this._drawCore(ctx, raw, centerX, centerY, scale, [rotX, rotY, rotZ]);

    if (!this.canvasVerified) {
      this.canvasVerified = this._verifyCanvasPaint(segments);
      if (!this.canvasVerified && this.svg) {
        this.useSvg = true;
        this._syncRenderMode();
        ctx.restore();
        this._renderSvg(segments);
        return;
      }
    }

    ctx.restore();
  }

  _verifyCanvasPaint(segments) {
    if (!this.ctx || !this.canvas || !segments.length) return false;
    const probes = segments.slice(0, 4).map((segment) => ({
      x: Math.round(((segment.va.x + segment.vb.x) * 0.5) * this.dpr),
      y: Math.round(((segment.va.y + segment.vb.y) * 0.5) * this.dpr),
    }));
    try {
      return probes.some(({ x, y }) => {
        const safeX = Math.max(0, Math.min(this.canvas.width - 1, x));
        const safeY = Math.max(0, Math.min(this.canvas.height - 1, y));
        const pixel = this.ctx.getImageData(safeX, safeY, 1, 1).data;
        return pixel[3] > 0;
      });
    } catch (_err) {
      return true;
    }
  }

  _renderSvg(segments) {
    if (!this.svg || !this.svgLines.length) return;
    this._syncRenderMode();
    this.svg.setAttribute('viewBox', '0 0 320 320');
    segments.forEach((segment, index) => {
      const pair = this.svgLines[index];
      if (!pair) return;
      const depthNorm = clamp01((segment.depth + 2) / 4);
      const glowAlpha = 0.24 + (depthNorm * 0.22);
      const coreAlpha = 0.68 + (depthNorm * 0.26);
      for (const line of [pair.glow, pair.core]) {
        line.setAttribute('x1', segment.va.x.toFixed(2));
        line.setAttribute('y1', segment.va.y.toFixed(2));
        line.setAttribute('x2', segment.vb.x.toFixed(2));
        line.setAttribute('y2', segment.vb.y.toFixed(2));
        line.setAttribute('vector-effect', 'non-scaling-stroke');
        line.setAttribute('fill', 'none');
      }
      pair.glow.setAttribute('stroke', `rgba(75, 213, 235, ${glowAlpha.toFixed(3)})`);
      pair.glow.setAttribute('stroke-width', '4.2');
      pair.core.setAttribute('stroke', `rgba(218, 250, 255, ${coreAlpha.toFixed(3)})`);
      pair.core.setAttribute('stroke-width', '1.45');
    });
  }
}

function renderStartupProgress() {
  const progress = Math.max(0, Math.min(100, startupState.progress));
  if (startupProgressFill) startupProgressFill.style.width = `${progress}%`;
  if (startupProgressLabel) startupProgressLabel.textContent = `${Math.round(progress)}%`;
  if (startupEtaLabel) {
    if (progress >= 99.5) {
      startupEtaLabel.textContent = 'Opening interface...';
    } else if (progress >= 6) {
      const elapsed = performance.now() - startupState.startedAt;
      const estimatedTotal = elapsed / Math.max(progress / 100, 0.06);
      const remaining = Math.max(0, estimatedTotal - elapsed);
      startupEtaLabel.textContent = `Backend setup: about ${formatBootTime(remaining)} remaining`;
    } else {
      startupEtaLabel.textContent = 'Estimating backend setup time...';
    }
  }
}

function setStartupStage(progress, status, detail = '') {
  startupState.progress = Math.max(0, Math.min(100, progress));
  if (startupStatus && status) startupStatus.textContent = status;
  if (startupDetail && detail) startupDetail.textContent = detail;
  renderStartupProgress();
}

function setStartupInstallVisible(visible) {
  startupInstallPanel?.classList.toggle('active', !!visible);
}

function renderRuntimeInstallState(state) {
  if (!state || typeof state !== 'object') return;
  const progress = Math.max(0, Math.min(100, Number(state.progress || 0)));
  const status = String(state.status || 'idle').toUpperCase();
  const lines = Array.isArray(state.log_lines) ? state.log_lines : Array.isArray(state.output) ? state.output : [];
  setTextIfPresent(runtimeStatusBadge, `STATUS: ${status}`, 'STATUS: IDLE');
  setTextIfPresent(runtimeStageText, `Stage: ${state.stage || 'Waiting'}`, 'Stage: Waiting');
  setTextIfPresent(runtimeEtaText, `ETA: ${state.eta_label || '--'}`, 'ETA: --');
  setTextIfPresent(runtimeDetailText, state.detail || 'Waiting for runtime activity...');
  setTextIfPresent(runtimeLogState, status, 'IDLE');
  setTextIfPresent(runtimeRecommendation, `Target: ${state.recommended_backend_label || latestSystemInfo?.recommended_backend_label || 'Unknown'}`, 'Target: Unknown');
  if (state.recommended_backend_label || latestSystemInfo?.recommended_backend_label) {
    setTextIfPresent(runtimeRecommendationLabel, state.recommended_backend_label || latestSystemInfo?.recommended_backend_label);
  }
  if (state.recommended_backend_reason || latestSystemInfo?.recommended_backend_reason) {
    setTextIfPresent(runtimeRecommendationReason, state.recommended_backend_reason || latestSystemInfo?.recommended_backend_reason);
  }
  if (runtimeProgressFill) runtimeProgressFill.style.width = `${progress}%`;
  if (runtimeProgressLabel) runtimeProgressLabel.textContent = `${Math.round(progress)}%`;
  if (runtimeInstallLog) {
    runtimeInstallLog.textContent = lines.length ? lines.join('\n') : 'Waiting for runtime activity...';
    runtimeInstallLog.scrollTop = runtimeInstallLog.scrollHeight;
  }
  if (settingsBackendLog) {
    settingsBackendLog.value = lines.length ? lines.join('\n') : 'Backend installer activity will appear here...';
    settingsBackendLog.scrollTop = settingsBackendLog.scrollHeight;
  }
}

function renderStartupInstallState(state) {
  if (!state || typeof state !== 'object') return;
  const lines = Array.isArray(state.log_lines) ? state.log_lines : Array.isArray(state.output) ? state.output : [];
  const status = String(state.status || 'idle').toUpperCase();
  if (startupInstallState) startupInstallState.textContent = status;
  if (startupInstallLog) {
    startupInstallLog.textContent = lines.length ? lines.join('\n') : 'Waiting for runtime checks...';
    startupInstallLog.scrollTop = startupInstallLog.scrollHeight;
  }
  if (lines.length) {
    startupState.installLogCount = lines.length;
  }
  if (state.stage || state.detail) {
    setStartupStage(
      Number(state.progress || startupState.progress),
      state.stage || 'Preparing runtime',
      state.detail || 'Installing required Python libraries and backend components',
    );
  }
  renderRuntimeInstallState(state);
}

function stopRuntimeInstallPolling() {
  if (runtimeInstallPollHandle) {
    window.clearInterval(runtimeInstallPollHandle);
    runtimeInstallPollHandle = null;
  }
}

async function fetchRuntimeInstallState() {
  try {
    const state = await apiGet('/api/startup/install_status');
    renderRuntimeInstallState(state);
    return state;
  } catch (_err) {
    return null;
  }
}

function startRuntimeInstallPolling() {
  stopRuntimeInstallPolling();
  fetchRuntimeInstallState();
  runtimeInstallPollHandle = window.setInterval(fetchRuntimeInstallState, 1500);
}

async function runStartupInstallSequence(initialState = null) {
  activatePage('runtime');
  setStartupInstallVisible(true);
  if (initialState) renderStartupInstallState(initialState);
  await apiPost('/api/startup/install', {});
  while (true) {
    const state = await apiGet('/api/startup/install_status');
    renderStartupInstallState(state);
    const status = String(state.status || 'idle').toLowerCase();
    if (status === 'completed') {
      return true;
    }
    if (status === 'error') {
      throw new Error(state.detail || 'Runtime setup failed');
    }
    await sleep(700);
  }
}

async function finishStartupSequence(status = 'Backend ready', detail = 'Interface online and ready to render') {
  // Give the hero animation enough time to register before transitioning away.
  const minimumSceneTime = 1900;
  const sceneElapsed = performance.now() - startupState.startedAt;
  if (sceneElapsed < minimumSceneTime) await sleep(minimumSceneTime - sceneElapsed);
  setStartupStage(100, status, detail);
  startupState.loader?.stop();
  await sleep(560);
  startupScreen?.classList.add('hidden');
  document.body.classList.remove('booting');
  await sleep(720);
  startupScreen?.remove();
}

function normalizeBackendName(label) {
  const key = (label || '').toLowerCase();
  if (key.includes('tensor')) return 'tensorrt';
  if (key.includes('ncnn')) return 'ncnn';
  if (key.includes('directml')) return 'directml';
  if (key.includes('pytorch')) return 'pytorch';
  if (key === 'auto') return 'auto';
  return key || 'auto';
}

function formatBackendLabel(key) {
  const map = {
    auto: 'Auto',
    pytorch: 'PyTorch',
    ncnn: 'NCNN',
    tensorrt: 'TensorRT',
    directml: 'DirectML',
  };
  return map[key] || key;
}

function setBackendOptions(installed = []) {
  if (!backendSelect) return;
  const options = ['auto', ...new Set(installed.map(normalizeBackendName).filter(Boolean))];
  backendSelect.innerHTML = '';
  options.forEach((key) => {
    const opt = document.createElement('option');
    opt.value = key;
    opt.textContent = formatBackendLabel(key);
    backendSelect.appendChild(opt);
  });
}

function renderSystemInfo(system) {
  latestSystemInfo = system || null;
  if (!systemInfoBox || !system) {
    renderRuntimeSystemInfo(system);
    return;
  }
  const backendList = (system.available_backends || []).join(', ') || 'None';
  const text = [
    `OS: ${system.os || 'Unknown'}`,
    `CPU: ${system.cpu || 'Unknown'}`,
    `RAM: ${system.ram || 'Unknown'}`,
    `Backends: ${backendList}`,
    `Disk Free: ${system.available_disk_gb ?? '?'} GB`,
    `Version: ${system.version || 'Unknown'}`,
    `Working Directory: ${system.working_directory || '-'}`,
    `Runtime Root: ${system.runtime_root || '-'}`,
    `Recommended Setup: ${system.recommended_backend_label || 'Unknown'}`,
    `Python Target: ${system.python_runtime || '-'}`,
  ].join('\n');
  systemInfoBox.value = text;
  renderRuntimeSystemInfo(system);
}

function setTextIfPresent(node, value, fallback = '-') {
  if (!node) return;
  const text = String(value ?? fallback).trim();
  node.textContent = text || fallback;
}

function setMeterState(node, available) {
  if (!node) return;
  node.style.width = available ? '100%' : '18%';
}

function renderRuntimeSystemInfo(system) {
  if (!system) return;
  const backendList = Array.isArray(system.available_backends) && system.available_backends.length
    ? system.available_backends.join(', ')
    : 'None detected yet';
  setTextIfPresent(runtimeRecommendation, `Target: ${system.recommended_backend_label || 'Unknown'}`, 'Target: Unknown');
  setTextIfPresent(runtimeRecommendationLabel, system.recommended_backend_label || 'Unknown');
  setTextIfPresent(runtimeRecommendationReason, system.recommended_backend_reason || 'No recommendation details available.');
  setTextIfPresent(runtimeBackendsText, backendList);
  setTextIfPresent(
    runtimeProbeHint,
    system.backend_probe_output
      ? `Probe: ${String(system.backend_probe_output).slice(0, 180)}`
      : 'The backend probe result updates after each install step.',
  );
  setTextIfPresent(runtimeWorkingDir, system.working_directory || '-');
  setTextIfPresent(runtimeRootDir, system.runtime_root || '-');
  setTextIfPresent(runtimeBackendPath, system.backend_script_path || '-');
  setTextIfPresent(runtimePythonPath, system.python_target_path || '-');
  setTextIfPresent(runtimeFfmpegPath, system.ffmpeg_path || '-');
  setTextIfPresent(runtimeTempPath, system.temp_directory || '-');
  setTextIfPresent(runtimeBackendReady, system.backend_script_available ? 'Ready' : 'Missing');
  setTextIfPresent(runtimePythonReady, system.python_available ? 'Ready' : 'Missing');
  setTextIfPresent(runtimeFfmpegReady, system.ffmpeg_available ? 'Ready' : 'Missing');
  setMeterState(runtimeBackendMeter, !!system.backend_script_available);
  setMeterState(runtimePythonMeter, !!system.python_available);
  setMeterState(runtimeFfmpegMeter, !!system.ffmpeg_available);
  const managerState = system.backend_management || {
    // Older API builds only expose rendering capabilities. Keep the manager
    // useful with those responses instead of showing an empty installed list.
    installed_backends: system.installed_backends || system.available_backends || [],
    torch_versions: ['2.9.0', '2.8.0', '2.7.0', '2.6.0'],
    pytorch_backends: [{ value: 'cuda', label: 'CUDA' }, { value: 'cpu', label: 'CPU' }],
    directml_supported: false,
    tensorrt_supported: true,
  };
  renderBackendManagerState(managerState, system);
}

function renderPythonInfo(info) {
  if (!info || typeof info !== 'object') return;
  setTextIfPresent(pythonStatusBadge, 'STATUS: Ready', 'STATUS: Idle');
  setTextIfPresent(pythonPipBadge, `pip: ${info.pip_available ? 'Ready' : 'Missing'}`, 'pip: Unknown');
  setTextIfPresent(pythonCommandPath, Array.isArray(info.python_command) ? info.python_command.join(' ') : info.python_label || '-');
  setTextIfPresent(pythonTargetPath, info.python_target_path || '-');
  setTextIfPresent(pythonPipLogPath, info.install_log_path || '-');
  if (pythonInfoOutput) {
    pythonInfoOutput.textContent = [
      info.python_detail || '',
      '',
      'pip:',
      info.pip_detail || '',
    ].join('\n').trim() || 'No Python runtime info available.';
  }
}

async function refreshPythonInfo() {
  try {
    setTextIfPresent(pythonStatusBadge, 'STATUS: Checking');
    const info = await apiGet('/api/python/info');
    renderPythonInfo(info);
    return info;
  } catch (err) {
    setTextIfPresent(pythonStatusBadge, 'STATUS: Error');
    if (pythonInfoOutput) pythonInfoOutput.textContent = `Python info failed: ${String(err)}`;
    return null;
  }
}

async function runPythonPipCommand(command) {
  const text = String(command || pythonPipCommand?.value || '').trim() || '--version';
  if (pythonPipCommand) pythonPipCommand.value = text;
  setTextIfPresent(pythonStatusBadge, 'STATUS: Running pip');
  setTextIfPresent(pythonCommandState, 'Running');
  if (pythonRunPipBtn) pythonRunPipBtn.disabled = true;
  if (pythonPipOutput) {
    pythonPipOutput.textContent = `Running pip ${text}...\n`;
  }
  try {
    const result = await apiPost('/api/python/pip', { command: text, timeout: 3600 });
    setTextIfPresent(pythonStatusBadge, result.ok ? 'STATUS: pip complete' : 'STATUS: pip failed');
    setTextIfPresent(pythonCommandState, result.ok ? 'Complete' : 'Failed');
    if (pythonPipOutput) {
      pythonPipOutput.textContent = [
        `$ ${result.command || `pip ${text}`}`,
        '',
        result.output || '(no output)',
      ].join('\n');
      pythonPipOutput.scrollTop = pythonPipOutput.scrollHeight;
    }
    await refreshPythonInfo();
    await refreshBackendState();
    return result;
  } catch (err) {
    setTextIfPresent(pythonStatusBadge, 'STATUS: pip error');
    setTextIfPresent(pythonCommandState, 'Error');
    if (pythonPipOutput) pythonPipOutput.textContent = `pip command failed: ${String(err)}`;
    return null;
  } finally {
    if (pythonRunPipBtn) pythonRunPipBtn.disabled = false;
  }
}

async function verifySelectedBackend() {
  const backend = backendSelect?.value || 'auto';
  setTextIfPresent(pythonStatusBadge, 'STATUS: Verifying backend');
  setTextIfPresent(pythonCommandState, 'Verifying');
  if (pythonPipOutput) pythonPipOutput.textContent = `Verifying ${backend}...\n`;
  try {
    const result = await apiPost('/api/actions/verify_backend', { backend });
    const lines = [`Backend: ${result.backend || backend}`, `Result: ${result.ok ? 'OK' : 'FAILED'}`, ''];
    (result.checks || []).forEach((check) => {
      lines.push(`${check.ok ? 'OK' : 'FAILED'} - ${check.name}`);
      if (check.detail) lines.push(String(check.detail));
      lines.push('');
    });
    if (pythonPipOutput) pythonPipOutput.textContent = lines.join('\n').trim();
    setTextIfPresent(pythonStatusBadge, result.ok ? 'STATUS: Backend verified' : 'STATUS: Backend problem');
    setTextIfPresent(pythonCommandState, result.ok ? 'Verified' : 'Failed');
    await refreshBackendState();
  } catch (err) {
    setTextIfPresent(pythonStatusBadge, 'STATUS: Verify error');
    setTextIfPresent(pythonCommandState, 'Error');
    if (pythonPipOutput) pythonPipOutput.textContent = `Backend verification failed: ${String(err)}`;
  }
}

function backendCategoryLabel(value) {
  return value ? String(value).replace('_', ' ') : 'not compatible';
}

function renderCustomModels(payload) {
  const folder = payload?.folder || '-';
  const models = Array.isArray(payload?.models) ? payload.models : [];
  setTextIfPresent(customModelsFolderPath, folder);
  setTextIfPresent(customModelsCount, `${models.length} model${models.length === 1 ? '' : 's'}`);
  if (!customModelsList) return;
  customModelsList.innerHTML = '';
  if (!models.length) {
    const empty = document.createElement('div');
    empty.className = 'custom-model-empty';
    empty.textContent = 'No custom models imported yet.';
    customModelsList.appendChild(empty);
    return;
  }
  models.forEach((model) => {
    const card = document.createElement('article');
    card.className = 'custom-model-card';

    const main = document.createElement('div');
    main.className = 'custom-model-main';
    const title = document.createElement('strong');
    title.textContent = model.name || 'Unnamed model';
    const meta = document.createElement('span');
    const kind = model.is_dir ? 'folder' : 'file';
    const scale = Number(model.scale || 1);
    meta.textContent = `${kind} - ${model.category || 'unknown'} - ${scale}x - ${model.size_label || '--'}`;
    const path = document.createElement('code');
    path.textContent = model.path || '';
    main.append(title, meta, path);

    const chips = document.createElement('div');
    chips.className = 'custom-model-chips';
    Object.entries(model.backend_categories || {}).forEach(([backend, category]) => {
      const chip = document.createElement('span');
      chip.className = `custom-model-chip${category ? ' ok' : ''}`;
      chip.textContent = `${category ? 'OK' : 'X'} ${backend}: ${backendCategoryLabel(category)}`;
      chips.appendChild(chip);
    });

    const actions = document.createElement('div');
    actions.className = 'custom-model-actions';
    const deleteBtn = document.createElement('button');
    deleteBtn.className = 'btn danger compact-btn';
    deleteBtn.type = 'button';
    deleteBtn.textContent = 'Delete';
    deleteBtn.addEventListener('click', () => deleteCustomModel(model.name));
    actions.appendChild(deleteBtn);

    card.append(main, chips, actions);
    customModelsList.appendChild(card);
  });
}

async function refreshCustomModels() {
  try {
    const payload = await apiGet('/api/custom_models');
    renderCustomModels(payload);
    await populateModelsForBackend();
    return payload;
  } catch (err) {
    if (customModelsList) {
      customModelsList.innerHTML = '';
      const error = document.createElement('div');
      error.className = 'custom-model-empty';
      error.textContent = `Could not load custom models: ${String(err)}`;
      customModelsList.appendChild(error);
    }
    return null;
  }
}

function importCustomModels(folder = false) {
  const picker = folder ? customModelFolderInput : customModelFilesInput;
  if (!picker) {
    appendDownloadLog('The model file picker is unavailable in this browser.');
    return;
  }
  picker.value = '';
  picker.click();
}

async function uploadCustomModelFiles(fileList, { directory = false, directTarget = null } = {}) {
  const files = [...(fileList || [])];
  if (!files.length) return;
  const allowed = new Set(['.pth', '.pt', '.onnx', '.safetensors', '.engine', '.bin', '.param', '.ckpt']);
  const accepted = files.filter((file) => allowed.has(file.name.slice(file.name.lastIndexOf('.')).toLowerCase()));
  const skipped = files.length - accepted.length;
  if (!accepted.length) {
    appendDownloadLog('No supported model files were selected. Choose a supported model file or an NCNN .bin/.param pair.');
    return;
  }
  const pairKey = (file) => (file.webkitRelativePath || file.name).replace(/\.(bin|param)$/i, '').toLowerCase();
  const binStems = new Set(accepted.filter((file) => /\.bin$/i.test(file.name)).map(pairKey));
  const paramStems = new Set(accepted.filter((file) => /\.param$/i.test(file.name)).map(pairKey));
  const unpaired = accepted.filter((file) => /\.bin$/i.test(file.name) ? !paramStems.has(pairKey(file))
    : /\.param$/i.test(file.name) && !binStems.has(pairKey(file)));
  const toUpload = accepted.filter((file) => !unpaired.includes(file));
  if (unpaired.length) {
    appendDownloadLog(`NCNN models need matching .bin and .param files. Skipped: ${unpaired.map((file) => file.name).join(', ')}`);
  }
  if (!toUpload.length) return;
  appendDownloadLog(`Importing ${toUpload.length} model file${toUpload.length === 1 ? '' : 's'}${directory ? ' from the selected folder' : ''}...`);
  const imported = [];
  try {
    for (const [index, file] of toUpload.entries()) {
      const relativePath = directory ? (file.webkitRelativePath || file.name) : file.name;
      const query = new URLSearchParams({ filename: file.name, relative_path: relativePath });
      const response = await fetch(`/api/custom_models/upload?${query}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/octet-stream' },
        body: file,
      });
      const payload = await response.json();
      if (!response.ok || !payload.ok) throw new Error(payload.error || `Could not import ${file.name}`);
      imported.push(payload.path);
      appendDownloadLog(`Imported ${index + 1}/${toUpload.length}: ${file.name}`);
    }
    await populateModelsForBackend();
    renderCustomModels(await apiGet('/api/custom_models'));
    if (directTarget && imported[0]) {
      directTarget.value = imported[0];
      directTarget.dispatchEvent(new Event('change', { bubbles: true }));
    }
    appendDownloadLog(`Imported ${imported.length} custom model file${imported.length === 1 ? '' : 's'}.`);
    if (skipped) appendDownloadLog(`Skipped ${skipped} unsupported file${skipped === 1 ? '' : 's'}.`);
    showToastNotification('Model import complete', `${imported.length} file${imported.length === 1 ? '' : 's'} added to the collection.`, true);
  } catch (err) {
    appendDownloadLog(`Custom model import failed: ${String(err)}`);
    showToastNotification('Model import failed', String(err));
  }
}

async function deleteCustomModel(name) {
  const modelName = String(name || '').trim();
  if (!modelName) return;
  const ok = window.confirm(`Delete custom model from the collection?\n\n${modelName}`);
  if (!ok) return;
  try {
    const payload = await apiPost('/api/actions/delete_custom_model', { name: modelName });
    renderCustomModels(payload);
    await populateModelsForBackend();
    appendDownloadLog(`Deleted custom model: ${modelName}`);
  } catch (err) {
    appendDownloadLog(`Delete custom model failed: ${String(err)}`);
  }
}

function fillSelectOptions(selectEl, items, mapItem) {
  if (!selectEl) return;
  const previousValue = selectEl.value;
  selectEl.innerHTML = '';
  (items || []).forEach((item) => {
    const normalized = mapItem ? mapItem(item) : { value: item, label: item };
    const option = document.createElement('option');
    option.value = normalized.value;
    option.textContent = normalized.label;
    selectEl.appendChild(option);
  });
  if ([...selectEl.options].some((option) => option.value === previousValue)) {
    selectEl.value = previousValue;
  }
}

function getBackendManagerTargetSelection() {
  const value = String(setBackendManagerTarget?.value || 'pytorch').toLowerCase();
  return value === 'tensorrt' ? 'tensorrt' : 'pytorch';
}

function syncBackendManagerVisibility() {
  const target = getBackendManagerTargetSelection();
  backendConfigPytorch?.classList.toggle('active', target === 'pytorch');
  backendConfigTensorRt?.classList.toggle('active', target === 'tensorrt');
}

function syncTensorRtSettingsMirrors() {
  if (setBackendTensorRTOptLevel && setTensorRTOptLevel) {
    setBackendTensorRTOptLevel.value = String(setTensorRTOptLevel.value || 3);
  }
  if (setBackendTensorRTDynamicEngine && setTensorRTDynamicEngine) {
    setBackendTensorRTDynamicEngine.value = setTensorRTDynamicEngine.checked ? 'true' : 'false';
  }
}

function applyTensorRtSettingsMirrorsToAdvanced() {
  if (setBackendTensorRTOptLevel && setTensorRTOptLevel) {
    setTensorRTOptLevel.value = setBackendTensorRTOptLevel.value || '3';
    setTensorRTOptLevel.dispatchEvent(new Event('change'));
  }
  if (setBackendTensorRTDynamicEngine && setTensorRTDynamicEngine) {
    setTensorRTDynamicEngine.checked = String(setBackendTensorRTDynamicEngine.value) === 'true';
    setTensorRTDynamicEngine.dispatchEvent(new Event('change'));
  }
}

function renderBackendManagerState(managerState, system = latestSystemInfo) {
  latestBackendManagerState = managerState || latestBackendManagerState;
  const state = managerState || latestBackendManagerState || {};
  const installed = Array.isArray(state.installed_backends) && state.installed_backends.length
    ? state.installed_backends.join(', ')
    : 'None';
  setTextIfPresent(settingsInstalledBackends, installed);
  setTextIfPresent(settingsRecommendedBackend, system?.recommended_backend_label || 'Unknown');
  setTextIfPresent(settingsBackendDiskFree, system?.available_disk_gb != null ? `${system.available_disk_gb} GB` : '--');
  setTextIfPresent(settingsBackendReason, system?.recommended_backend_reason || 'No backend recommendation is available yet.');
  setTextIfPresent(
    settingsBackendSupportNote,
    state.directml_supported
      ? 'Backend install and delete actions are available below.'
      : 'DirectML stays disabled here too, matching the upstream app.',
  );

  fillSelectOptions(
    setBackendTorchVersion,
    state.torch_versions || ['2.9.0', '2.8.0', '2.6.0'],
    (item) => ({ value: String(item), label: String(item) }),
  );
  fillSelectOptions(
    setBackendTensorRtTorchVersion,
    state.torch_versions || ['2.9.0', '2.8.0', '2.6.0'],
    (item) => ({ value: String(item), label: String(item) }),
  );
  fillSelectOptions(
    setBackendTorchBackend,
    state.pytorch_backends || [{ value: 'cuda', label: 'CUDA' }],
    (item) => ({ value: String(item.value), label: String(item.label) }),
  );
  if (setBackendTensorRtBackend) {
    setBackendTensorRtBackend.value = 'CUDA (required)';
  }
  syncTensorRtSettingsMirrors();
  syncBackendManagerVisibility();

  const installedKeys = Array.isArray(state.installed_backends) ? state.installed_backends : [];
  const hasNcnn = installedKeys.includes('ncnn');
  const hasTorch = installedKeys.includes('pytorch');
  const hasTensorRt = installedKeys.includes('tensorrt');
  if (settingsUninstallNcnnBtn) settingsUninstallNcnnBtn.disabled = !hasNcnn;
  if (settingsUninstallTorchBtn) settingsUninstallTorchBtn.disabled = !hasTorch;
  if (settingsUninstallTensorRtBtn) settingsUninstallTensorRtBtn.disabled = !hasTensorRt;
  if (settingsInstallTensorRtBtn) settingsInstallTensorRtBtn.disabled = state.tensorrt_supported === false;
  if (settingsInstallDirectMlBtn) settingsInstallDirectMlBtn.disabled = true;
  if (settingsUninstallDirectMlBtn) settingsUninstallDirectMlBtn.disabled = true;
}

function appendSettingsBackendLog(message) {
  if (!settingsBackendLog) return;
  const previous = settingsBackendLog.value || '';
  settingsBackendLog.value = `${previous}\n[${new Date().toLocaleTimeString()}] ${message}`.trim();
  settingsBackendLog.scrollTop = settingsBackendLog.scrollHeight;
}

function appendDownloadLog(message) {
  if (!downloadLog) return;
  const previous = downloadLog.value || '';
  downloadLog.value = `${previous}\n[${new Date().toLocaleTimeString()}] ${message}`.trim();
  downloadLog.scrollTop = downloadLog.scrollHeight;
}

function appendEngineLogLine(message) {
  if (!engineLog) return;
  const text = String(message || '').trim();
  if (!text) return;
  if ((engineLog.value || '').trim() === 'Engine logs will appear here...') {
    engineLog.value = '';
  }
  const nextLine = `[${new Date().toLocaleTimeString()}] ${text}`;
  const existing = engineLog.value ? engineLog.value.split('\n').filter(Boolean) : [];
  existing.push(nextLine);
  const trimmed = existing.length > ENGINE_LOG_MAX_LINES
    ? existing.slice(existing.length - ENGINE_LOG_MAX_LINES)
    : existing;
  engineLog.value = `${trimmed.join('\n')}\n`;
  engineLog.scrollTop = engineLog.scrollHeight;
}

function log(msg) {
  appendEngineLogLine(msg);
}

function showToastNotification(title, message, done = false) {
  if (!etaToasts) return;
  const toast = document.createElement('div');
  toast.className = `eta-toast${done ? ' done' : ''}`;
  const heading = String(title || 'Notice').trim() || 'Notice';
  const text = String(message || '').trim();
  toast.innerHTML = `<strong>${heading}</strong>${text ? ` ${text}` : ''}`;
  etaToasts.prepend(toast);
  setTimeout(() => {
    toast.style.opacity = '0';
    toast.style.transform = 'translateX(14px)';
  }, done ? 1800 : 1300);
  setTimeout(() => toast.remove(), done ? 2300 : 1700);
}

function showEtaToast(message, done = false) {
  showToastNotification('ETA', message, done);
}

function reportRenderStartError(error, context = 'Render') {
  const message = String(error?.message || error || 'Unknown error');
  if (error?.code === 'BACKEND_NOT_INSTALLED') {
    activatePage('runtime');
    appendDownloadLog(message);
    showToastNotification('Rendering backend required', message, true);
    return;
  }
  showEtaToast(`${context} failed: ${message}`, true);
}

function primeDesktopNotifications() {
  try {
    if (typeof window === 'undefined' || !('Notification' in window)) return;
    if (Notification.permission !== 'default') return;
    Notification.requestPermission().catch(() => {});
  } catch (_err) {
  }
}

function showDesktopNotification(title, message, tag = 'rve-render') {
  try {
    if (typeof window === 'undefined' || !('Notification' in window)) return;
    if (Notification.permission !== 'granted') return;
    const notification = new Notification(String(title || 'REAL Video Enhancer'), {
      body: String(message || '').trim(),
      tag,
      renotify: true,
      silent: false,
    });
    window.setTimeout(() => {
      try {
        notification.close();
      } catch (_err) {
      }
    }, 7000);
  } catch (_err) {
  }
}

function notifyRenderEvent(title, message, { done = false, desktop = false, tag = 'rve-render' } = {}) {
  showToastNotification(title, message, done);
  if (desktop) {
    showDesktopNotification(title, message, tag);
  }
}

function setLiveEtaToast(message, state = 'running') {
  if (!etaToasts) return;
  const normalizedState = String(state || 'running').toLowerCase();
  const text = String(message || '--').trim();
  if (!text || text === '--' || normalizedState !== 'running') {
    if (liveEtaToast) {
      liveEtaToast.remove();
      liveEtaToast = null;
    }
    return;
  }
  if (!liveEtaToast || !etaToasts.contains(liveEtaToast)) {
    liveEtaToast = document.createElement('div');
    liveEtaToast.className = 'eta-toast live';
    etaToasts.prepend(liveEtaToast);
  }
  liveEtaToast.innerHTML = `<strong>ETA</strong> ${text}`;
}

function setLiveModelDownloadToast(status) {
  const indicator = document.getElementById('modelDownloadIndicator');
  const indicatorTitle = document.getElementById('modelDownloadTitle');
  const indicatorPercent = document.getElementById('modelDownloadPercent');
  const indicatorFill = document.getElementById('modelDownloadFill');
  const indicatorDetail = document.getElementById('modelDownloadDetail');
  const active = !!(status?.active ?? status?.download_active);
  const label = String(status?.label || status?.download_label || 'model').trim() || 'model';
  const progress = Number(status?.progress ?? status?.download_progress ?? 0);
  const progressText = Number.isFinite(progress) ? `${Math.round(progress)}%` : '--';
  const detail = String(status?.detail || status?.download_detail || '').trim();
  if (indicator) indicator.hidden = !active;
  if (active) {
    if (indicatorTitle) indicatorTitle.textContent = `${status?.stage || status?.download_stage || 'Downloading'} · ${label}`;
    if (indicatorPercent) indicatorPercent.textContent = progressText;
    if (indicatorFill) indicatorFill.style.width = `${Math.max(0, Math.min(100, progress || 0))}%`;
    if (indicatorDetail) indicatorDetail.textContent = detail || 'Preparing model download…';
  }
  if (!etaToasts) return;
  if (!active) {
    if (liveModelDownloadToast) {
      liveModelDownloadToast.remove();
      liveModelDownloadToast = null;
    }
    lastModelDownloadLabel = '';
    return;
  }
  if (lastModelDownloadLabel !== label) {
    showToastNotification('Download', `Downloading model: ${label}`, false);
    lastModelDownloadLabel = label;
  }
  if (!liveModelDownloadToast || !etaToasts.contains(liveModelDownloadToast)) {
    liveModelDownloadToast = document.createElement('div');
    liveModelDownloadToast.className = 'eta-toast live';
    etaToasts.prepend(liveModelDownloadToast);
  }
  liveModelDownloadToast.replaceChildren();
  const strong = document.createElement('strong');
  strong.textContent = 'Downloading model';
  liveModelDownloadToast.append(strong, ` ${progressText} - ${label}`);
  if (detail) {
    liveModelDownloadToast.append(document.createElement('br'), detail);
  }
}

function notifyEtaUpdate(etaValue, statusValue) {
  const eta = String(etaValue || '--');
  const status = String(statusValue || '').toLowerCase();
  setLiveEtaToast(eta, status);
  if (status !== 'running') return;
  if (eta === '--') return;
  const now = Date.now();
  if (eta !== lastEtaToastValue && (now - lastEtaToastAt) >= 10000) {
    showEtaToast(eta);
    lastEtaToastValue = eta;
    lastEtaToastAt = now;
  }
}

function setStatus(etaValue, fpsValue, stateValue) {
  if (etaText) etaText.textContent = `ETA: ${etaValue}`;
  if (fpsText) fpsText.textContent = `Gen FPS: ${fpsValue}`;
  if (statusText) statusText.textContent = `STATUS: ${stateValue}`;
  generationState.eta = etaValue;
  generationState.status = stateValue;
  const parsed = Number.parseFloat(String(fpsValue));
  generationState.fps = Number.isFinite(parsed) ? parsed : null;
  updatePreviewHud();
}

function formatPercent(value) {
  if (value === null || value === undefined || Number.isNaN(Number(value))) return '--';
  return `${Number(value).toFixed(1)}%`;
}

function formatMem(used, total) {
  if (
    used === null || used === undefined || total === null || total === undefined
    || Number.isNaN(Number(used)) || Number.isNaN(Number(total))
  ) {
    return '--';
  }
  return `${Number(used).toFixed(1)} / ${Number(total).toFixed(1)} GB`;
}

function updatePreviewHud() {
  if (hudEta) hudEta.textContent = generationState.eta;
  if (hudGenFps) hudGenFps.textContent = generationState.fps === null ? '--' : `${generationState.fps.toFixed(1)}`;
  if (hudCpu) hudCpu.textContent = formatPercent(latestRuntimeMetrics.cpu_percent);
  if (hudGpu) hudGpu.textContent = formatPercent(latestRuntimeMetrics.gpu_util_percent);
  if (hudRam) hudRam.textContent = formatMem(latestRuntimeMetrics.ram_used_gb, latestRuntimeMetrics.ram_total_gb);
  if (hudVram) hudVram.textContent = formatMem(latestRuntimeMetrics.vram_used_gb, latestRuntimeMetrics.vram_total_gb);
}

function setLivePreviewText(text) {
  if (livePreviewLabel) livePreviewLabel.textContent = text;
  if (livePreviewExpandedLabel) livePreviewExpandedLabel.textContent = text;
  livePreviewBox?.classList.toggle('loading', String(text || '').toLowerCase().includes('working'));
  livePreviewExpanded?.classList.toggle('loading', String(text || '').toLowerCase().includes('working'));
}

function previewIsEnabled() {
  return processPreviewEnabled?.checked !== false && setPreviewEnabled?.checked !== false;
}

function setPreviewEnabledState(enabled, source = null) {
  const value = !!enabled;
  if (processPreviewEnabled && source !== processPreviewEnabled) processPreviewEnabled.checked = value;
  if (setPreviewEnabled && source !== setPreviewEnabled) setPreviewEnabled.checked = value;
  if (!value) {
    releasePreviewFrameObjectUrl();
    if (firstFrameObjectUrl) {
      URL.revokeObjectURL(firstFrameObjectUrl);
      firstFrameObjectUrl = null;
    }
    setPreviewFrameSource('');
    setLivePreviewText('Preview Disabled');
  } else if (!livePreviewImage?.getAttribute('src')) {
    setLivePreviewText('Live Preview - Idle');
  }
}

function setPreviewFrameSource(url) {
  if (livePreviewImage) livePreviewImage.src = url || '';
  if (livePreviewExpandedImage) livePreviewExpandedImage.src = url || '';
  if (url) {
    livePreviewBox?.classList.remove('loading');
    livePreviewExpanded?.classList.remove('loading');
  }
}

function releasePreviewFrameObjectUrl() {
  if (previewFrameObjectUrl) {
    URL.revokeObjectURL(previewFrameObjectUrl);
    previewFrameObjectUrl = null;
  }
}

function formatDuration(totalSeconds) {
  const sec = Math.max(0, Math.round(Number(totalSeconds) || 0));
  const h = Math.floor(sec / 3600);
  const m = Math.floor((sec % 3600) / 60);
  const s = sec % 60;
  return `${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
}

function formatMaybeNumber(value, digits = 1) {
  const num = Number(value);
  if (!Number.isFinite(num) || num <= 0) return '--';
  return Number.isInteger(num) ? String(num) : num.toFixed(digits);
}

function detectScaleFromModelLabel(label) {
  const match = String(label || '').match(/\((\d+)X\)/i);
  if (!match) return 1;
  const scale = Number.parseInt(match[1], 10);
  return Number.isFinite(scale) && scale > 0 ? scale : 1;
}

function currentUpscaleScale() {
  if (!toggleUpscale?.checked) return 1;
  const manual = Number(upscaleScale?.value || 0);
  if (Number.isFinite(manual) && manual > 0) return manual;
  return detectScaleFromModelLabel(upscaleModel?.value || '');
}

function currentInterpolationFactor() {
  if (!toggleInterpolate?.checked) return 1;
  const factor = Number(interpolateFactor?.value || 2);
  return Number.isFinite(factor) && factor > 0 ? factor : 1;
}

function computedOutputResolution() {
  const width = Number(currentVideoInfo.width || 0);
  const height = Number(currentVideoInfo.height || 0);
  const scale = currentUpscaleScale();
  return {
    width: width > 0 ? Math.round(width * scale) : 0,
    height: height > 0 ? Math.round(height * scale) : 0,
  };
}

function computedOutputFrames() {
  const frames = Number(currentVideoInfo.frames || 0);
  if (frames <= 0) return 0;
  return Math.max(frames, Math.round(frames * currentInterpolationFactor()));
}

function computedOutputFps() {
  const fps = Number(currentVideoInfo.fps || 0);
  if (fps <= 0) return 0;
  if (toggleSloMo?.checked) return fps;
  return fps * currentInterpolationFactor();
}

function setPreviewMetaValue(node, text) {
  if (!node) return;
  node.textContent = text || '--';
}

function updatePreviewProgress() {
  const totalFrames = computedOutputFrames();
  const status = String(generationState.status || '').toLowerCase();
  const downloading = !!generationState.downloadActive;
  let percent = 0;
  if (downloading) {
    percent = Math.max(0, Math.min(100, Number(generationState.downloadProgress || 0)));
  } else if (status === 'completed') {
    percent = 100;
  } else if (totalFrames > 0 && generationState.frame > 0) {
    percent = Math.max(0, Math.min(100, (generationState.frame / totalFrames) * 100));
  }
  if (previewProgressFill) previewProgressFill.style.width = `${percent}%`;
  previewProgressFill?.classList.toggle('downloading', downloading);
  if (previewProgressText) previewProgressText.textContent = `${Math.round(percent)}%`;
  if (previewProgressDetail) {
    previewProgressDetail.textContent = downloading
      ? `Downloading model: ${generationState.downloadLabel || 'model'}`
      : '';
  }
}

function updatePreviewDetails() {
  const inputFps = Number(currentVideoInfo.fps || 0);
  const outputFpsValue = computedOutputFps();
  const outputRes = computedOutputResolution();
  const outputFramesValue = computedOutputFrames();
  const inputResolution = currentVideoInfo.width > 0 && currentVideoInfo.height > 0
    ? `${currentVideoInfo.width}x${currentVideoInfo.height}`
    : '--';
  const outputResolution = outputRes.width > 0 && outputRes.height > 0
    ? `${outputRes.width}x${outputRes.height}`
    : '--';
  const inputFramesValue = Number(currentVideoInfo.frames || 0) > 0 ? String(currentVideoInfo.frames) : '--';
  const outputFramesText = outputFramesValue > 0 ? String(outputFramesValue) : '--';
  setPreviewMetaValue(
    previewInfoFps,
    inputFps > 0 ? `${formatMaybeNumber(inputFps, 2)} -> ${formatMaybeNumber(outputFpsValue, 2)}` : '--',
  );
  setPreviewMetaValue(previewInfoResolution, `${inputResolution} -> ${outputResolution}`);
  setPreviewMetaValue(previewInfoFrames, `${inputFramesValue} -> ${outputFramesText}`);
  setPreviewMetaValue(previewInfoBitrate, currentVideoInfo.bitrate || '--');
  setPreviewMetaValue(previewInfoEncoder, videoEncoder?.value || '--');
  setPreviewMetaValue(previewInfoContainer, (setVideoContainer?.value || videoContainer?.value || currentVideoInfo.container || '--').toLowerCase());
  setPreviewMetaValue(previewInfoColorSpace, currentVideoInfo.color_space || 'Unknown');
  setPreviewMetaValue(previewInfoPixelFormat, setVideoPixelFormat?.value || videoPixelFormat?.value || currentVideoInfo.pixel_format || '--');
  if (outputFps) outputFps.textContent = outputFpsValue > 0 ? formatMaybeNumber(outputFpsValue, 2) : '--';
  if (videoFrames) videoFrames.textContent = outputFramesValue > 0 ? String(outputFramesValue) : '--';
  updatePreviewProgress();
}

async function refreshPreviewFrame(renderState = 'running') {
  if (!previewIsEnabled() || previewFrameFetchInFlight) return;
  previewFrameFetchInFlight = true;
  try {
    const stamp = Date.now();
    const response = await fetch(`/api/render/preview_frame?t=${stamp}`, { cache: 'no-store' });
    if (!response.ok) {
      const terminal = ['completed', 'stopped', 'error'].includes(renderState);
      setLivePreviewText(terminal ? 'No rendered frame was produced' : 'Waiting for first enhanced frame...');
      return;
    }
    const blob = await response.blob();
    if (!blob || blob.size === 0) {
      const terminal = ['completed', 'stopped', 'error'].includes(renderState);
      setLivePreviewText(terminal ? 'No rendered frame was produced' : 'Waiting for first enhanced frame...');
      return;
    }
    releasePreviewFrameObjectUrl();
    previewFrameObjectUrl = URL.createObjectURL(blob);
    setPreviewFrameSource(previewFrameObjectUrl);
    clearPreviewLabel();
  } catch (_err) {
  } finally {
    previewFrameFetchInFlight = false;
  }
}

function clearPreviewLabel() {
  setLivePreviewText('');
}

async function loadInputFirstFrame(path) {
  if (!path || !previewIsEnabled()) return;
  try {
    setLivePreviewText('Loading thumbnail...');
    const response = await fetch(`/api/video/first_frame?path=${encodeURIComponent(path)}`);
    if (!response.ok) {
      setLivePreviewText('Thumbnail unavailable');
      return;
    }
    const blob = await response.blob();
    if (!blob || blob.size === 0) {
      setLivePreviewText('Thumbnail unavailable');
      return;
    }
    if (firstFrameObjectUrl) URL.revokeObjectURL(firstFrameObjectUrl);
    firstFrameObjectUrl = URL.createObjectURL(blob);
    releasePreviewFrameObjectUrl();
    setPreviewFrameSource(firstFrameObjectUrl);
    clearPreviewLabel();
  } catch (_err) {
    setLivePreviewText('Thumbnail unavailable');
  }
}

function getModelAvailability(statusMap, name) {
  if (!statusMap || !Object.prototype.hasOwnProperty.call(statusMap, name)) return 'unknown';
  return statusMap[name] === true ? 'installed' : statusMap[name] === false ? 'missing' : 'unknown';
}

function compactModelLabel(name) {
  return String(name)
    .replace(/\s*\(Slow Model, Animation\/Realistic\)/i, ' · Slow · Animation / realistic')
    .replace(/\s*\(Animation\/Realistic\)/i, ' · Animation / realistic')
    .replace(/\s*\(Slow Model, Realistic\/General\)/i, ' · Slow · Realistic / general')
    .replace(/\s*\(Fast Model, Realistic only\)/i, ' · Fast · Realistic')
    .replace(/\s*\(Slowest RIFE Model, Animation\)/i, ' · Slowest · Animation')
    .replace(/\s*\(Fastest Model\)/i, ' · Fastest')
    .replace(/\s*\(Smoothest Model\)/i, ' · Smoothest')
    .replace(/\s*\(Latest General Model, Recommended\)/i, ' · Recommended')
    .replace(/\s*\(Latest LITE model\)/i, ' · Latest Lite')
    .replace(/\s*\(Low-High Quality Source\)/i, ' · Any quality')
    .replace(/\s*\((High|Medium|Low) Quality Source\)/gi, (_, quality) => ` · ${quality} quality`)
    .replace(/\s*\((\d+)X\)/gi, ' · ×$1')
    .replace(/\s*\((Fast|Slow)\)/gi, ' · $1')
    .replace(/\s*\((Realistic|Animation|Old Animation|HD Input)\)/gi, ' · $1')
    .replace(/\s*\((Helps with text warping)\)/gi, '')
    .replace(/\s*\(([^)]*)\)/g, ' · $1');
}

function updateModelAvailabilityHint(selectEl, statusMap) {
  if (!selectEl) return;
  let hint = selectEl.parentElement?.querySelector('.model-install-status');
  if (!hint && selectEl.parentElement) {
    hint = document.createElement('small');
    hint.className = 'model-install-status is-unknown';
    hint.setAttribute('aria-live', 'polite');
    selectEl.insertAdjacentElement('afterend', hint);
  }
  if (!hint) return;
  const availability = getModelAvailability(statusMap, selectEl.value);
  const selectedOption = selectEl.selectedOptions?.[0];
  selectEl.title = selectedOption?.title || selectEl.value;
  const messages = {
    installed: ['✓ Installed', 'is-installed'],
    missing: ['✕ Not installed · download required', 'is-missing'],
    unknown: ['Installation status unavailable', 'is-unknown'],
  };
  const [message, className] = messages[availability];
  hint.textContent = message;
  hint.className = `model-install-status ${className}`;
  hint.title = availability === 'unknown'
    ? 'The local model check did not return a result.'
    : '';
}

function setOptions(selectEl, items, statusMap = null) {
  if (!selectEl) return;
  selectEl._modelStatusMap = statusMap;
  const previousValue = selectEl.value;
  selectEl.innerHTML = '';
  if (!items || !items.length) {
    const opt = document.createElement('option');
    opt.textContent = 'No models available';
    selectEl.appendChild(opt);
    selectEl.disabled = true;
    return;
  }
  selectEl.disabled = false;
  items.forEach((name) => {
    const opt = document.createElement('option');
    opt.value = name;
    const availability = getModelAvailability(statusMap, name);
    const indicator = availability === 'installed' ? '✅ ' : availability === 'missing' ? '❌ ' : '○ ';
    opt.textContent = `${indicator}${compactModelLabel(name)}`;
    opt.dataset.availability = availability;
    opt.title = name;
    selectEl.appendChild(opt);
  });
  if ([...selectEl.options].some((option) => option.value === previousValue)) {
    selectEl.value = previousValue;
  }
  updateModelAvailabilityHint(selectEl, statusMap);
  if (!selectEl.dataset.availabilityListener) {
    selectEl.addEventListener('change', () => {
      updateModelAvailabilityHint(selectEl, selectEl._modelStatusMap || null);
    });
    selectEl.dataset.availabilityListener = 'true';
  }
}

function selectedBackendKey() {
  const value = (backendSelect?.value || 'auto').toLowerCase();
  if (value === 'auto') return 'pytorch';
  return value;
}

function applyModelPayload(models) {
  if (!models) return;
  const status = models.model_status || {};
  setOptions(interpolateModel, models.interpolate, status.interpolate);
  setOptions(upscaleModel, models.upscale, status.upscale);
  setOptions(decompressModel, models.decompress, status.decompress);
  setOptions(denoiseModel, models.denoise, status.denoise);
  setOptions(deblurModel, models.deblur, status.deblur);
  updatePreviewDetails();
}

function populateModelsForBackendFallback() {
  const backend = selectedBackendKey();
  const models = MODEL_CATALOG[backend] || MODEL_CATALOG.pytorch;
  applyModelPayload(models);
  refreshFallbackModelStatuses(models);
  log(`Model list updated for backend (fallback): ${backend}`);
}

async function refreshFallbackModelStatuses(models) {
  try {
    const modelNames = Object.fromEntries(['interpolate', 'upscale', 'decompress', 'denoise', 'deblur'].map((category) => [
      category,
      Array.from(({ interpolate: interpolateModel, upscale: upscaleModel, decompress: decompressModel, denoise: denoiseModel, deblur: deblurModel })[category]?.options || []).map((option) => option.value),
    ]));
    const response = await apiPost('/api/models/status', { models: modelNames, backend: selectedBackendKey() });
    applyModelPayload({ ...models, model_status: response.model_status || {} });
  } catch (_err) {
    // Preserve the UI's unknown marker when the backend cannot inspect weights.
  }
}

async function populateModelsForBackend() {
  const backend = backendSelect?.value || 'auto';
  try {
    const payload = await apiGet(`/api/models?backend=${encodeURIComponent(backend)}`);
    applyModelPayload(payload);
    log(`Model list updated for backend: ${payload.backend}`);
    return;
  } catch (err) {
    log(`Model API unavailable, using fallback: ${String(err)}`);
  }
  populateModelsForBackendFallback();
}

function selectedModelRequirements() {
  if ((processingEngine?.value || 'rve').toLowerCase() === 'animescripter') return {};
  const selections = {};
  if (toggleInterpolate?.checked) selections.interpolate = interpolateModel?.value || '';
  if (toggleUpscale?.checked) selections.upscale = upscaleModel?.value || '';
  if (toggleDecompress?.checked) selections.decompress = decompressModel?.value || '';
  if (toggleDenoise?.checked) selections.denoise = denoiseModel?.value || '';
  if (toggleDeblur?.checked) selections.deblur = deblurModel?.value || '';
  return selections;
}

async function runModelDownload(payload, description = 'Model download') {
  const started = await apiPost('/api/models/download', payload);
  if (!started?.id) throw new Error(started?.error || 'Could not start model download.');
  appendDownloadLog(`${description} started.`);
  while (true) {
    const status = await apiGet(`/api/models/download_status?id=${encodeURIComponent(started.id)}`);
    setLiveModelDownloadToast(status);
    if (status.status === 'completed') {
      appendDownloadLog(status.detail || 'Selected models are ready.');
      await populateModelsForBackend();
      return status;
    }
    if (status.status === 'error') throw new Error(status.error || status.detail || 'Model download failed.');
    await sleep(500);
  }
}

async function prepareModelsForQueue() {
  const selections = selectedModelRequirements();
  if (!Object.keys(selections).length) return;
  const selectedMissing = Object.entries(selections).some(([category, label]) => {
    const element = ({ interpolate: interpolateModel, upscale: upscaleModel, decompress: decompressModel, denoise: denoiseModel, deblur: deblurModel })[category];
    return label && getModelAvailability(element?._modelStatusMap || null, label) !== 'installed';
  });
  if (!selectedMissing) return;
  showToastNotification('Preparing render', 'Downloading selected model weights…');
  await runModelDownload({ backend: selectedBackendKey(), selections }, 'Selected model download');
}

async function bootstrapFromBackend(reportStage = null) {
  try {
    reportStage?.(38, 'Connecting to backend bridge...', 'Waiting for Python backend API');
    const payload = await apiGet('/api/bootstrap');
    reportStage?.(58, 'Reading installed runtimes...', 'Collecting system info, backends and model catalogs');
    renderSystemInfo(payload.system);
    setBackendOptions(payload.system?.available_backends || []);
    setProcessAvailability((payload.system?.available_backends || []).length > 0);
    const initialBackend = payload.models?.backend || 'pytorch';
    if (backendSelect) {
      const hasOption = [...backendSelect.options].some((opt) => opt.value === initialBackend);
      backendSelect.value = hasOption ? initialBackend : 'auto';
    }
    reportStage?.(74, 'Applying saved workspace state...', 'Restoring models, toggles and encoder settings');
    applyModelPayload(payload.models);
    applySettingsToUI(payload.settings || {});
    syncFeatureModules();
    syncAdvancedMirrors();
    if (setVideoContainer && videoContainer) videoContainer.value = setVideoContainer.value;
    if (setVideoPixelFormat && videoPixelFormat) videoPixelFormat.value = setVideoPixelFormat.value;
    updateEncoderPreview();
    log('Connected to Python backend API');
    renderRuntimeInstallState(payload.startup?.install_state || {});
    refreshPythonInfo();
    refreshCustomModels();
    if (payload.startup?.needs_runtime_install) {
      log('Runtime setup is required before the backend can render.');
    }
    reportStage?.(86, 'Backend bridge connected', 'Preview and processing controls are being synchronized');
    return { ok: true, payload };
  } catch (err) {
    setBackendOptions(['pytorch', 'ncnn', 'tensorrt', 'directml']);
    renderSystemInfo(null);
    log(`Backend API not available, running in fallback mode: ${String(err)}`);
    populateModelsForBackendFallback();
    reportStage?.(72, 'Backend fallback mode', 'Could not reach live Python API, using fallback data');
    return { ok: false, error: err };
  }
}

async function refreshBackendState() {
  try {
    const system = await apiPost('/api/actions/refresh_backends');
    setProcessAvailability((system.available_backends || []).length > 0);
    renderSystemInfo(system);
    setBackendOptions(system.available_backends || []);
    await fetchRuntimeInstallState();
    await refreshPythonInfo();
    await refreshCustomModels();
    appendDownloadLog('Backends refreshed from runtime.');
    await populateModelsForBackend();
    return system;
  } catch (err) {
    setProcessAvailability(false);
    appendDownloadLog(`Backend refresh failed: ${String(err)}`);
    return null;
  }
}

async function runBackendManagerAction(action, backend) {
  const actionLabel = action === 'install' ? 'Installing' : 'Deleting';
  const normalizedBackend = String(backend || '').toLowerCase();
  const useTensorRtConfig = normalizedBackend.includes('tensor')
    ? true
    : normalizedBackend.includes('torch')
      ? false
      : getBackendManagerTargetSelection() === 'tensorrt';
  const selectedTorchVersion = useTensorRtConfig
    ? (setBackendTensorRtTorchVersion?.value || setBackendTorchVersion?.value || '2.9.0')
    : (setBackendTorchVersion?.value || '2.9.0');
  const selectedTorchBackend = useTensorRtConfig
    ? 'cuda'
    : (setBackendTorchBackend?.value || 'cuda');
  appendDownloadLog(`${actionLabel} ${backend} backend...`);
  appendSettingsBackendLog(`${actionLabel} ${backend} backend...`);
  activatePage('settings');
  try {
    const payload = await apiPost('/api/actions/manage_backend', {
      action,
      backend,
      torch_version: selectedTorchVersion,
      pytorch_backend: selectedTorchBackend,
    });
    renderRuntimeInstallState(payload?.state || {});
    startRuntimeInstallPolling();
    while (true) {
      const state = await apiGet('/api/startup/install_status');
      renderStartupInstallState(state);
      const status = String(state.status || 'idle').toLowerCase();
      if (status === 'completed') {
        appendDownloadLog(state.detail || `${backend} ${action} completed.`);
        appendSettingsBackendLog(state.detail || `${backend} ${action} completed.`);
        await refreshBackendState();
        return true;
      }
      if (status === 'error') {
        throw new Error(state.detail || `${backend} ${action} failed.`);
      }
      await sleep(700);
    }
  } catch (err) {
    appendDownloadLog(`${actionLabel} ${backend} failed: ${String(err)}`);
    appendSettingsBackendLog(`${actionLabel} ${backend} failed: ${String(err)}`);
    throw err;
  } finally {
    startRuntimeInstallPolling();
  }
}

async function runRuntimeSetupFromUI() {
  activatePage('runtime');
  appendDownloadLog('Recommended runtime setup started.');
  try {
    const initial = await apiPost('/api/startup/install', {});
    renderRuntimeInstallState(initial);
    startRuntimeInstallPolling();
    while (true) {
      const state = await apiGet('/api/startup/install_status');
      renderStartupInstallState(state);
      const status = String(state.status || 'idle').toLowerCase();
      if (status === 'completed') {
        appendDownloadLog(state.detail || 'Recommended runtime setup completed.');
        return await refreshBackendState();
      }
      if (status === 'error') {
        throw new Error(state.detail || 'Recommended runtime setup failed');
      }
      await sleep(700);
    }
  } catch (err) {
    appendDownloadLog(`Recommended runtime setup failed: ${String(err)}`);
    throw err;
  } finally {
    startRuntimeInstallPolling();
  }
}

function installButtonsAsDesktopBridge() {
  installAnimeScripterBtn?.addEventListener('click', installAnimeScripterFromUI);
  installRecommendedBtn?.addEventListener('click', async () => {
    try {
      await runRuntimeSetupFromUI();
    } catch (err) {
      log(`Install recommended failed: ${String(err)}`);
    }
  });
  downloadInterpolationModelsBtn?.addEventListener('click', async () => {
    appendDownloadLog('Downloading interpolation models for current backend...');
    try {
      const payload = await runModelDownload({
        backend: backendSelect?.value || 'auto',
        categories: ['interpolate'],
      }, 'Interpolation model download');
      appendDownloadLog(payload.detail || 'Interpolation models processed.');
    } catch (err) {
      appendDownloadLog(`Interpolation model download failed: ${String(err)}`);
    }
  });
  downloadUpscaleModelsBtn?.addEventListener('click', async () => {
    appendDownloadLog('Downloading upscale models for current backend...');
    try {
      const payload = await runModelDownload({
        backend: backendSelect?.value || 'auto',
        categories: ['upscale', 'decompress', 'denoise', 'deblur'],
      }, 'Upscale and restoration model download');
      appendDownloadLog(payload.detail || 'Upscale/restoration models processed.');
    } catch (err) {
      appendDownloadLog(`Upscale model download failed: ${String(err)}`);
    }
  });
  downloadAllModelsBtn?.addEventListener('click', async () => {
    appendDownloadLog('Downloading every available model for the current backend...');
    try {
      const payload = await runModelDownload({
        backend: backendSelect?.value || 'auto',
        categories: ['interpolate', 'upscale', 'decompress', 'denoise', 'deblur', 'scene_change'],
      }, 'Complete model download');
      appendDownloadLog(payload.detail || 'All models processed.');
    } catch (err) {
      appendDownloadLog(`Download all models failed: ${String(err)}`);
    }
  });
  refreshBackendsBtn?.addEventListener('click', refreshBackendState);
  runtimeStartBtn?.addEventListener('click', async () => {
    try {
      await runRuntimeSetupFromUI();
    } catch (err) {
      log(`Runtime setup failed: ${String(err)}`);
    }
  });
  runtimeRefreshBtn?.addEventListener('click', refreshBackendState);
  runtimeOpenFolderBtn?.addEventListener('click', async () => {
    try {
      await apiPost('/api/actions/open_rve_folder', {});
    } catch (err) {
      appendDownloadLog(`Open runtime folder failed: ${String(err)}`);
    }
  });
  runtimeShowDownloadBtn?.addEventListener('click', () => activatePage('download'));
  pythonRefreshBtn?.addEventListener('click', refreshPythonInfo);
  pythonPipVersionBtn?.addEventListener('click', () => runPythonPipCommand('--version'));
  pythonPipListBtn?.addEventListener('click', () => runPythonPipCommand('list'));
  pythonPipFreezeBtn?.addEventListener('click', () => runPythonPipCommand('freeze'));
  pythonVerifyBackendBtn?.addEventListener('click', verifySelectedBackend);
  pythonRunPipBtn?.addEventListener('click', () => runPythonPipCommand());
  pythonPipCommand?.addEventListener('keydown', (event) => {
    if (event.key === 'Enter') {
      event.preventDefault();
      runPythonPipCommand();
    }
  });
  pythonClearOutputBtn?.addEventListener('click', () => {
    if (pythonPipOutput) pythonPipOutput.textContent = 'pip output will appear here...';
    setTextIfPresent(pythonCommandState, 'Idle');
  });
  openRveFolderBtn?.addEventListener('click', async () => {
    try {
      await apiPost('/api/actions/open_rve_folder', {});
    } catch (err) {
      appendDownloadLog(`Open RVE folder failed: ${String(err)}`);
    }
  });
  importCustomModelsBtn?.addEventListener('click', () => importCustomModels(false));
  importCustomModelFolderBtn?.addEventListener('click', () => importCustomModels(true));
  customModelFilesInput?.addEventListener('change', () => uploadCustomModelFiles(customModelFilesInput.files));
  customModelFolderInput?.addEventListener('change', () => uploadCustomModelFiles(customModelFolderInput.files, { directory: true }));
  customModelSingleInput?.addEventListener('change', () => {
    const target = customModelSingleInput.dataset.target === 'interpolate' ? setCustomInterpolateModel : setCustomUpscaleModel;
    uploadCustomModelFiles(customModelSingleInput.files, { directTarget: target });
  });
  const openCustomModelPathPicker = (category) => {
    if (!customModelSingleInput) return;
    customModelSingleInput.value = '';
    customModelSingleInput.dataset.target = category;
    customModelSingleInput.click();
  };
  refreshCustomModelsBtn?.addEventListener('click', refreshCustomModels);
  openCustomModelsFolderBtn?.addEventListener('click', async () => {
    try {
      const payload = await apiPost('/api/actions/open_custom_models_folder', {});
      appendDownloadLog(payload.ok ? `Opened custom models folder: ${payload.path}` : 'Could not open custom models folder.');
      await refreshCustomModels();
    } catch (err) {
      appendDownloadLog(`Open custom models folder failed: ${String(err)}`);
    }
  });
  resetSettingsBtn?.addEventListener('click', async () => {
    try {
      const payload = await apiPost('/api/settings/reset', {});
      applySettingsToUI(payload.settings || {});
      syncFeatureModules();
      syncAdvancedMirrors();
      updateEncoderPreview();
      appendDownloadLog('Settings reset to defaults.');
    } catch (err) {
      appendDownloadLog(`Reset settings failed: ${String(err)}`);
    }
  });
  pickCustomInterpolateModelBtn?.addEventListener('click', async () => {
    openCustomModelPathPicker('interpolate');
  });
  pickCustomUpscaleModelBtn?.addEventListener('click', async () => {
    openCustomModelPathPicker('upscale');
  });
  settingsInstallRecommendedBtn?.addEventListener('click', async () => {
    try {
      await runRuntimeSetupFromUI();
    } catch (err) {
      log(`Install recommended failed: ${String(err)}`);
    }
  });
  settingsRefreshBackendsBtn?.addEventListener('click', refreshBackendState);
  settingsVerifyBackendBtn?.addEventListener('click', async () => {
    activatePage('python');
    await verifySelectedBackend();
  });
  setBackendManagerTarget?.addEventListener('change', () => {
    syncBackendManagerVisibility();
  });
  setBackendTensorRtTorchVersion?.addEventListener('change', () => {
    if (getBackendManagerTargetSelection() === 'tensorrt' && setBackendTorchVersion) {
      setBackendTorchVersion.value = setBackendTensorRtTorchVersion.value || '2.9.0';
      setBackendTorchVersion.dispatchEvent(new Event('change'));
    }
  });
  setBackendTensorRTOptLevel?.addEventListener('change', () => {
    applyTensorRtSettingsMirrorsToAdvanced();
  });
  setBackendTensorRTDynamicEngine?.addEventListener('change', () => {
    applyTensorRtSettingsMirrorsToAdvanced();
  });
  setTensorRTOptLevel?.addEventListener('change', () => {
    syncTensorRtSettingsMirrors();
  });
  setTensorRTDynamicEngine?.addEventListener('change', () => {
    syncTensorRtSettingsMirrors();
  });
  settingsInstallNcnnBtn?.addEventListener('click', async () => {
    try {
      await runBackendManagerAction('install', 'ncnn');
    } catch (err) {
      log(`Install NCNN failed: ${String(err)}`);
    }
  });
  settingsUninstallNcnnBtn?.addEventListener('click', async () => {
    try {
      await runBackendManagerAction('uninstall', 'ncnn');
    } catch (err) {
      log(`Delete NCNN failed: ${String(err)}`);
    }
  });
  settingsInstallTorchBtn?.addEventListener('click', async () => {
    try {
      if (setBackendManagerTarget) {
        setBackendManagerTarget.value = 'pytorch';
        syncBackendManagerVisibility();
      }
      await runBackendManagerAction('install', 'torch');
    } catch (err) {
      log(`Install PyTorch failed: ${String(err)}`);
    }
  });
  settingsUninstallTorchBtn?.addEventListener('click', async () => {
    try {
      await runBackendManagerAction('uninstall', 'torch');
    } catch (err) {
      log(`Delete PyTorch failed: ${String(err)}`);
    }
  });
  settingsInstallTensorRtBtn?.addEventListener('click', async () => {
    try {
      if (setBackendManagerTarget) {
        setBackendManagerTarget.value = 'tensorrt';
        syncBackendManagerVisibility();
      }
      await runBackendManagerAction('install', 'tensorrt');
    } catch (err) {
      log(`Install TensorRT failed: ${String(err)}`);
    }
  });
  settingsUninstallTensorRtBtn?.addEventListener('click', async () => {
    try {
      await runBackendManagerAction('uninstall', 'tensorrt');
    } catch (err) {
      log(`Delete TensorRT failed: ${String(err)}`);
    }
  });
  settingsInstallDirectMlBtn?.addEventListener('click', () => {
    appendSettingsBackendLog('DirectML is not implemented yet in the upstream app and remains disabled here.');
  });
  settingsUninstallDirectMlBtn?.addEventListener('click', () => {
    appendSettingsBackendLog('DirectML is not implemented yet in the upstream app and remains disabled here.');
  });
}

function basename(path) {
  if (!path) return '';
  const normalized = String(path).replace(/\\/g, '/');
  const parts = normalized.split('/');
  return parts[parts.length - 1] || normalized;
}

function setControlValue(control, value) {
  if (!control) return;
  if (value === undefined || value === null) return;
  control.value = String(value);
}

function setControlChecked(control, value) {
  if (!control) return;
  control.checked = !!value;
}

function readAdvancedSettingsFromUI() {
  const inputPath = inputPathField?.value || '';
  const normalizedInputPath = String(inputPath).replace(/\\/g, '/');
  const inputFolderIndex = normalizedInputPath.lastIndexOf('/');
  const inputFolder = inputFolderIndex > 0 ? normalizedInputPath.slice(0, inputFolderIndex).replace(/\//g, '\\') : '';
  return {
    use_same_output_folder_as_input: !!setUseSameOutputFolder?.checked,
    scene_change_detection_enabled: !!setSceneDetectEnabled?.checked,
    scene_change_detection_method: setSceneDetectMethod?.value || 'pyscenedetect',
    scene_change_detection_threshold: Number(setSceneDetectThreshold?.value || 3.5),
    scene_detect_model: setSceneDetectModel?.value.trim() || '',
    auto_border_cropping: !!setAutoBorderCropping?.checked,
    precision: setPrecision?.value || 'auto',
    tensorrt_optimization_level: Number(setTensorRTOptLevel?.value || 3),
    dynamic_tensorrt_engine: !!setTensorRTDynamicEngine?.checked,
    uhd_mode: !!setUHDMode?.checked,
    auto_hdr_mode: !!setAutoHDRMode?.checked,
    dynamic_scaled_optical_flow: !!setDynamicScaledFlow?.checked,
    ensemble: !!setEnsemble?.checked,
    pytorch_gpu_id: Number(setPytorchGpuId?.value || 0),
    ncnn_gpu_id: Number(setNcnnGpuId?.value || 0),
    preview_enabled: previewIsEnabled(),
    discord_rich_presence: !!setDiscordRPC?.checked,
    custom_interpolate_model: setCustomInterpolateModel?.value || '',
    custom_upscale_model: setCustomUpscaleModel?.value || '',
    video_container: setVideoContainer?.value || 'mkv',
    video_pixel_format: setVideoPixelFormat?.value || 'yuv420p',
    benchmark_mode: !!advBenchmarkMode?.checked,
    tiling_enabled: !!advTiling?.checked,
    tile_size: Number(advTileSize?.value || 0),
    overlap: Math.max(0, Number(advTileOverlap?.value || 0)),
    overwrite: !!advOverwrite?.checked,
    device: advDevice?.value || 'auto',
    output_to_mpv: !!advOutputToMpv?.checked,
    merge_subtitles: !!advMergeSubtitles?.checked,
    motion_blur: !!advMotionBlur?.checked,
    motion_blur_shutter_angle: Math.min(360, Math.max(0, Number(advMotionBlurShutterAngle?.value || 180))),
    motion_blur_strength: advMotionBlurStrength?.value || 'gaussian_sym',
    motion_blur_gamma: !!advMotionBlurGamma?.checked,
    deduplicate: !!advDeduplicate?.checked,
    deduplicate_threshold: Math.min(1, Math.max(0, Number(advDeduplicateThreshold?.value || 0.9))),
    video_quality: String(videoQualityPreset?.value || 'High').trim().replace(/\s+/g, '_'),
    output_folder_location: setOutputFolderLocation?.value || (outputPathField?.value && outputPathField.value !== 'No folder selected'
      ? outputPathField.value
      : ''),
    render_start_time: renderStartTime?.value || '',
    render_end_time: renderEndTime?.value || '',
    last_input_folder_location: inputFolder,
  };
}

function readEncodingSettingsFromUI() {
  const customEncoderOptions = customEncoder?.value.trim();
  return {
    ...(customEncoderOptions ? { custom_encoder: customEncoderOptions } : {}),
    video_tune: videoTune?.value || 'none',
    keyframe_interval: Math.max(1, Math.min(300, Number(keyframeInterval?.value || 48))),
    video_encoder_preset: videoEncoder?.value || 'libx264',
    video_encoder_speed: videoEncoderSpeed?.value || 'medium',
    audio_encoder_preset: audioEncoder?.value || 'copy_audio',
    subtitle_encoder_preset: subtitleEncoder?.value || 'copy_subtitle',
    audio_bitrate: audioBitrate?.value || '192k',
    video_container: setVideoContainer?.value || videoContainer?.value || 'mkv',
    video_pixel_format: setVideoPixelFormat?.value || videoPixelFormat?.value || 'yuv420p',
    crf: videoCrf?.value || '18',
    video_quality: String(videoQualityPreset?.value || 'High').trim().replace(/\s+/g, '_'),
  };
}

function collectWebSettings() {
  return {
    backend: backendSelect?.value || 'auto',
    engine: processingEngine?.value || 'rve',
    animescripter: {
      smooth_dedup: !!document.getElementById('tasSmoothDedup')?.checked,
      depth: !!document.getElementById('tasDepth')?.checked,
      segment: !!document.getElementById('tasSegment')?.checked,
      stabilize: !!document.getElementById('tasStabilize')?.checked,
      depth_quality: document.getElementById('tasDepthQuality')?.value || 'low',
    },
    toggles: {
      interpolate: !!toggleInterpolate?.checked,
      upscale: !!toggleUpscale?.checked,
      slomo: !!toggleSloMo?.checked,
      decompress: !!toggleDecompress?.checked,
      denoise: !!toggleDenoise?.checked,
      deblur: !!toggleDeblur?.checked,
    },
    processing_options: {
      denoise_strength: Number(document.getElementById('denoiseStrength')?.value || 5),
      denoise_temporal: document.getElementById('denoiseTemporal')?.value || 'Off',
      denoise_keep_film_grain: !!document.getElementById('denoiseFilmGrain')?.checked,
      deblur_strength: Number(document.getElementById('deblurStrength')?.value || 6),
      deblur_edge_recovery: Number(document.getElementById('deblurEdgeRecovery')?.value || 38),
      deblur_ghosting_guard: document.getElementById('deblurGhostingGuard')?.value || 'Auto',
    },
    advanced: readAdvancedSettingsFromUI(),
    encoding: readEncodingSettingsFromUI(),
    backend_management: {
      backend_manager_target: getBackendManagerTargetSelection(),
      pytorch_version: setBackendTorchVersion?.value || setBackendTensorRtTorchVersion?.value || '2.9.0',
      pytorch_backend: (setBackendTorchBackend?.value || 'cuda').toUpperCase(),
    },
    render_profile: document.getElementById('renderProfile')?.value || 'Balanced',
    decompression: {
      compression_bias: document.getElementById('compressionBias')?.value || 'High/Medium Source',
      strength: Math.min(10, Math.max(1, Number(document.getElementById('decompressStrength')?.value || 6))),
      banding_reduction: document.getElementById('decompressBanding')?.value || 'Balanced',
    },
  };
}

function applySettingsToUI(settings) {
  if (!settings || typeof settings !== 'object') return;
  const processingOptions = settings.processing_options || {};
  if ('denoise_strength' in processingOptions) setControlValue(document.getElementById('denoiseStrength'), processingOptions.denoise_strength);
  if ('denoise_temporal' in processingOptions) setControlValue(document.getElementById('denoiseTemporal'), processingOptions.denoise_temporal);
  if ('denoise_keep_film_grain' in processingOptions) setControlChecked(document.getElementById('denoiseFilmGrain'), processingOptions.denoise_keep_film_grain);
  if ('deblur_strength' in processingOptions) setControlValue(document.getElementById('deblurStrength'), processingOptions.deblur_strength);
  if ('deblur_edge_recovery' in processingOptions) setControlValue(document.getElementById('deblurEdgeRecovery'), processingOptions.deblur_edge_recovery);
  if ('deblur_ghosting_guard' in processingOptions) setControlValue(document.getElementById('deblurGhostingGuard'), processingOptions.deblur_ghosting_guard);
  if (backendSelect && settings.backend) backendSelect.value = settings.backend;
  if (processingEngine && settings.engine) processingEngine.value = settings.engine;
  updateProcessingEngineUI();
  const tas = settings.animescripter || {};
  setControlChecked(document.getElementById('tasSmoothDedup'), tas.smooth_dedup);
  setControlChecked(document.getElementById('tasDepth'), tas.depth);
  setControlChecked(document.getElementById('tasSegment'), tas.segment);
  setControlChecked(document.getElementById('tasStabilize'), tas.stabilize);
  setControlValue(document.getElementById('tasDepthQuality'), tas.depth_quality);
  setControlValue(document.getElementById('renderProfile'), settings.render_profile);
  if (settings.toggles && typeof settings.toggles === 'object') {
    setControlChecked(toggleInterpolate, settings.toggles.interpolate);
    setControlChecked(toggleUpscale, settings.toggles.upscale);
    setControlChecked(toggleSloMo, settings.toggles.slomo);
    setControlChecked(toggleDecompress, settings.toggles.decompress);
    setControlChecked(toggleDenoise, settings.toggles.denoise);
    setControlChecked(toggleDeblur, settings.toggles.deblur);
  }
  const advanced = settings.advanced || settings;
  const encoding = settings.encoding || settings;
  const backendManagement = settings.backend_management || settings;
  setControlChecked(setUseSameOutputFolder, advanced.use_same_output_folder_as_input || advanced.use_same_output_folder_as_input_file_enabled);
  setControlChecked(setSceneDetectEnabled, advanced.scene_change_detection_enabled);
  setControlValue(setSceneDetectMethod, advanced.scene_change_detection_method);
  setControlValue(setSceneDetectThreshold, advanced.scene_change_detection_threshold);
  setControlValue(setSceneDetectModel, advanced.scene_detect_model);
  setControlChecked(setAutoBorderCropping, advanced.auto_border_cropping);
  setControlValue(setPrecision, advanced.precision);
  setControlValue(setTensorRTOptLevel, advanced.tensorrt_optimization_level);
  setControlChecked(setTensorRTDynamicEngine, advanced.dynamic_tensorrt_engine);
  setControlChecked(setUHDMode, advanced.uhd_mode);
  setControlChecked(setAutoHDRMode, advanced.auto_hdr_mode);
  setControlChecked(setDynamicScaledFlow, advanced.dynamic_scaled_optical_flow);
  setControlChecked(setEnsemble, advanced.ensemble);
  setControlValue(setPytorchGpuId, advanced.pytorch_gpu_id);
  setControlValue(setNcnnGpuId, advanced.ncnn_gpu_id);
  setPreviewEnabledState(advanced.preview_enabled !== false);
  setControlChecked(setDiscordRPC, advanced.discord_rich_presence);
  setControlValue(setCustomInterpolateModel, advanced.custom_interpolate_model);
  setControlValue(setCustomUpscaleModel, advanced.custom_upscale_model);
  setControlValue(setVideoContainer, advanced.video_container || encoding.video_container);
  setControlValue(setVideoPixelFormat, advanced.video_pixel_format || encoding.video_pixel_format);
  setControlChecked(advBenchmarkMode, advanced.benchmark_mode);
  setControlChecked(advTiling, advanced.tiling_enabled);
  setControlValue(advTileSize, advanced.tile_size);
  setControlValue(advTileOverlap, advanced.overlap ?? advanced.tile_overlap ?? 10);
  setControlChecked(advOverwrite, advanced.overwrite);
  setControlValue(advDevice, advanced.device || 'auto');
  setControlChecked(advOutputToMpv, advanced.output_to_mpv);
  setControlChecked(advMergeSubtitles, advanced.merge_subtitles !== false);
  setControlChecked(advMotionBlur, advanced.motion_blur);
  setControlChecked(advMotionBlurGamma, advanced.motion_blur_gamma !== false);
  setControlValue(advMotionBlurShutterAngle, advanced.motion_blur_shutter_angle ?? 180);
  setControlValue(advMotionBlurStrength, advanced.motion_blur_strength || 'gaussian_sym');
  setControlChecked(advDeduplicate, advanced.deduplicate);
  setControlValue(advDeduplicateThreshold, advanced.deduplicate_threshold ?? 0.9);
  setControlValue(videoQualityPreset, String(advanced.video_quality || '').replace('_', ' '));
  setControlValue(videoEncoder, encoding.video_encoder_preset || advanced.encoder);
  setControlValue(videoEncoderSpeed, encoding.video_encoder_speed || advanced.video_encoder_speed);
  setControlValue(audioEncoder, encoding.audio_encoder_preset || advanced.audio_encoder);
  setControlValue(subtitleEncoder, encoding.subtitle_encoder_preset || advanced.subtitle_encoder);
  setControlValue(audioBitrate, encoding.audio_bitrate || advanced.audio_bitrate);
  setControlValue(videoCrf, encoding.crf || videoCrf?.value || '18');
  setControlValue(customEncoder, encoding.custom_encoder || '');
  setControlValue(videoTune, encoding.video_tune || 'none');
  setControlValue(keyframeInterval, encoding.keyframe_interval ?? 48);
  setControlValue(renderStartTime, advanced.render_start_time);
  setControlValue(renderEndTime, advanced.render_end_time);
  const target = String(backendManagement.backend_manager_target || 'pytorch').toLowerCase();
  setControlValue(setBackendManagerTarget, target === 'tensorrt' ? 'tensorrt' : 'pytorch');
  setControlValue(setBackendTorchVersion, backendManagement.pytorch_version || '2.9.0');
  setControlValue(setBackendTensorRtTorchVersion, backendManagement.pytorch_version || '2.9.0');
  setControlValue(setBackendTorchBackend, String(backendManagement.pytorch_backend || 'CUDA').toLowerCase());
  const decompression = settings.decompression || {};
  setControlValue(document.getElementById('compressionBias'), decompression.compression_bias);
  setControlValue(document.getElementById('decompressStrength'), decompression.strength);
  setControlValue(document.getElementById('decompressBanding'), decompression.banding_reduction);
  syncTensorRtSettingsMirrors();
  syncBackendManagerVisibility();
  if (outputPathField) {
    outputPathField.value = advanced.output_folder_location || 'No folder selected';
  }
  setControlValue(setOutputFolderLocation, advanced.output_folder_location || '');
}

function syncAdvancedMirrors() {
  setControlChecked(advDynamicScaledFlowMirror, !!setDynamicScaledFlow?.checked);
  setControlChecked(advEnsembleMirror, !!setEnsemble?.checked);
  setControlValue(advTensorRTOptMirror, setTensorRTOptLevel?.value || 3);
}

function setupInputOutputPickers() {
  setSelectOutputFolderBtn?.addEventListener('click', async () => {
    try {
      const payload = await apiPost('/api/dialog/select_output');
      if (!payload?.path) return;
      setOutputFolderLocation.value = payload.path;
      if (outputPathField && (outputPathField.value === 'No folder selected' || !setUseSameOutputFolder?.checked)) {
        outputPathField.value = payload.path;
        outputPathField.dispatchEvent(new Event('change'));
      }
      setOutputFolderLocation.dispatchEvent(new Event('change'));
    } catch (err) {
      const message = err?.message || String(err);
      appendSettingsBackendLog(`Default output folder dialog failed: ${message}`);
      showToastNotification('Could not select output folder', message);
    }
  });

  selectInputVideoBtn?.addEventListener('click', async () => {
    try {
      const payload = await apiPost('/api/dialog/select_input');
      const selected = payload?.path;
      if (!selected) return;
      if (inputPathField) inputPathField.value = selected;
      if (setUseSameOutputFolder?.checked) {
        const normalized = String(selected).replace(/\\/g, '/');
        const idx = normalized.lastIndexOf('/');
        const folder = idx > 0 ? normalized.slice(0, idx).replace(/\//g, '\\') : '';
        if (folder && outputPathField) {
          outputPathField.value = folder;
          outputPathField.dispatchEvent(new Event('change'));
        }
      }
      const info = payload?.video_info || {};
      currentVideoInfo.width = Number(info.width || 0);
      currentVideoInfo.height = Number(info.height || 0);
      currentVideoInfo.fps = Number(info.fps || 0);
      currentVideoInfo.frames = Number(info.frames || 0);
      currentVideoInfo.duration_seconds = Number(info.duration_seconds || 0);
      currentVideoInfo.bitrate = info.bitrate || null;
      currentVideoInfo.codec = info.codec || null;
      currentVideoInfo.container = info.container || basename(selected).split('.').pop() || null;
      currentVideoInfo.color_space = info.color_space || null;
      currentVideoInfo.pixel_format = info.pixel_format || null;
      if (videoRes) {
        const w = Number(info.width || 0);
        const h = Number(info.height || 0);
        videoRes.textContent = (w > 0 && h > 0) ? `${w}x${h}` : '--';
      }
      if (videoDuration) videoDuration.textContent = info.duration_seconds ? formatDuration(info.duration_seconds) : '--';
      if (videoFrames) videoFrames.textContent = info.frames ? String(info.frames) : '--';
      if (outputFps) outputFps.textContent = info.fps ? String(Math.round(Number(info.fps) * 100) / 100) : '--';
      if (previewIsEnabled()) clearPreviewLabel();
      updatePreviewDetails();
      log(`Input video selected: ${selected}`);
      inputPathField?.dispatchEvent(new Event('change'));
    } catch (err) {
      const message = err?.message || String(err);
      appendDownloadLog(`Input dialog failed: ${message}`);
      showToastNotification('Could not import video', message);
    }
  });

  inputPathField?.addEventListener('change', async () => {
    const path = inputPathField.value || '';
    if (!path || path === 'No file selected' || !previewIsEnabled()) return;
    await loadInputFirstFrame(path);
  });

  batchSelectBtn?.addEventListener('click', () => {
    (async () => {
      try {
        const payload = await apiPost('/api/dialog/select_inputs');
        const paths = Array.isArray(payload?.paths) ? payload.paths : [];
        if (!paths.length) {
          appendDownloadLog(payload?.detail || 'No supported videos were found in the selected folder.');
          showToastNotification('No valid videos found', payload?.detail || 'Choose a folder containing supported video files.');
          return;
        }
        paths.forEach((path) => addQueueItem(basename(path), path));
        const first = paths[0];
        if (inputPathField) inputPathField.value = first;
        if (setUseSameOutputFolder?.checked) {
          const normalized = String(first).replace(/\\/g, '/');
          const idx = normalized.lastIndexOf('/');
          const folder = idx > 0 ? normalized.slice(0, idx).replace(/\//g, '\\') : '';
          if (folder && outputPathField) {
            outputPathField.value = folder;
            outputPathField.dispatchEvent(new Event('change'));
          }
        }
        const info = payload?.video_info || {};
        currentVideoInfo.width = Number(info.width || 0);
        currentVideoInfo.height = Number(info.height || 0);
        currentVideoInfo.fps = Number(info.fps || 0);
        currentVideoInfo.frames = Number(info.frames || 0);
        currentVideoInfo.duration_seconds = Number(info.duration_seconds || 0);
        currentVideoInfo.bitrate = info.bitrate || null;
        currentVideoInfo.codec = info.codec || null;
        currentVideoInfo.container = info.container || basename(first).split('.').pop() || null;
        if (videoRes) videoRes.textContent = info.width && info.height ? `${info.width}x${info.height}` : '--';
        if (videoDuration) videoDuration.textContent = info.duration_seconds ? formatDuration(info.duration_seconds) : '--';
        if (videoFrames) videoFrames.textContent = info.frames ? String(info.frames) : '--';
        if (outputFps) outputFps.textContent = info.fps ? String(Math.round(Number(info.fps) * 100) / 100) : '--';
        if (previewIsEnabled()) clearPreviewLabel();
        updatePreviewDetails();
        inputPathField?.dispatchEvent(new Event('change'));
        appendDownloadLog(`Added ${paths.length} video(s) to the render queue.`);
        if (payload?.detail) appendDownloadLog(payload.detail);
        log(`Batch selection added ${paths.length} video(s)`);
      } catch (err) {
        const message = err?.message || String(err);
        appendDownloadLog(`Batch select failed: ${message}`);
        showToastNotification('Could not select batch videos', message);
      }
    })();
  });

  selectOutputFolderBtn?.addEventListener('click', async () => {
    try {
      const payload = await apiPost('/api/dialog/select_output');
      const selected = payload?.path;
      if (!selected) return;
      if (outputPathField) {
        outputPathField.value = selected;
        outputPathField.dispatchEvent(new Event('change'));
      }
      log(`Output folder selected: ${selected}`);
    } catch (err) {
      const message = err?.message || String(err);
      appendDownloadLog(`Output dialog failed: ${message}`);
      showToastNotification('Could not select output folder', message);
    }
  });

  openOutputFolderBtn?.addEventListener('click', async () => {
    const path = outputPathField?.value || '';
    if (!path || path === 'No folder selected') return;
    try {
      await apiPost('/api/dialog/open_output', { path });
    } catch (err) {
      appendDownloadLog(`Open folder failed: ${String(err)}`);
    }
  });
}

function installSettingsAutosave() {
  const watchedIds = [
    'processingEngine',
    'tasSmoothDedup',
    'tasDepth',
    'tasSegment',
    'tasStabilize',
    'tasDepthQuality',
    'toggleInterpolate',
    'toggleUpscale',
    'toggleSloMo',
    'toggleDecompress',
    'toggleDenoise',
    'toggleDeblur',
    'backendSelect',
    'renderProfile',
    'videoEncoder',
    'videoEncoderSpeed',
    'audioEncoder',
    'audioBitrate',
    'subtitleEncoder',
    'customEncoder',
    'videoTune',
    'keyframeInterval',
    'videoQualityPreset',
    'videoCrf',
    'renderStartTime',
    'renderEndTime',
    'interpolateFactor',
    'upscaleScale',
    'interpolateModel',
    'upscaleModel',
    'decompressModel',
    'compressionBias',
    'decompressStrength',
    'decompressBanding',
    'denoiseModel',
    'deblurModel',
    'denoiseStrength',
    'denoiseTemporal',
    'denoiseFilmGrain',
    'deblurStrength',
    'deblurEdgeRecovery',
    'deblurGhostingGuard',
    'setUseSameOutputFolder',
    'setOutputFolderLocation',
    'setSceneDetectEnabled',
    'setAutoBorderCropping',
    'setSceneDetectMethod',
    'setSceneDetectThreshold',
    'setSceneDetectModel',
    'setVideoContainer',
    'setVideoPixelFormat',
    'setPrecision',
    'setNcnnGpuId',
    'setPytorchGpuId',
    'setTensorRTOptLevel',
    'setAutoHDRMode',
    'setTensorRTDynamicEngine',
    'setUHDMode',
    'setDynamicScaledFlow',
    'setEnsemble',
    'processPreviewEnabled',
    'setPreviewEnabled',
    'setDiscordRPC',
    'setCustomInterpolateModel',
    'setCustomUpscaleModel',
    'setBackendManagerTarget',
    'setBackendTorchVersion',
    'setBackendTorchBackend',
    'setBackendTensorRtTorchVersion',
    'setBackendTensorRTOptLevel',
    'setBackendTensorRTDynamicEngine',
    'advBenchmarkMode',
    'advTiling',
    'advTileSize',
    'advTileOverlap',
    'advOverwrite',
    'advDevice',
    'advOutputToMpv',
    'advMergeSubtitles',
    'advMotionBlur',
    'advMotionBlurGamma',
    'advMotionBlurShutterAngle',
    'advMotionBlurStrength',
    'advDeduplicate',
    'advDeduplicateThreshold',
    'advDynamicScaledFlowMirror',
    'advEnsembleMirror',
    'advTensorRTOptMirror',
    'outputPathField',
  ];
  const watched = watchedIds
    .map((id) => document.getElementById(id))
    .filter(Boolean);
  const save = async () => {
    const settings = collectWebSettings();
    try {
      await apiPost('/api/settings', { settings });
    } catch (_err) {
    }
  };
  watched.forEach((node) => {
    node.addEventListener('change', save);
    if (node.tagName === 'INPUT' && (node.type === 'number' || node.type === 'text')) {
      try {
        node.addEventListener('input', save);
      } catch (_err) {
      }
    }
  });
}

function installPreviewToggleSync() {
  const handleToggle = async (event) => {
    const source = event.currentTarget;
    const enabled = source?.checked !== false;
    setPreviewEnabledState(enabled, source);
    if (enabled) {
      const inputPath = inputPathField?.value || '';
      if (inputPath && inputPath !== 'No file selected') {
        await loadInputFirstFrame(inputPath);
      }
    }
  };
  processPreviewEnabled?.addEventListener('change', handleToggle);
  setPreviewEnabled?.addEventListener('change', handleToggle);
}

function toggleModule(checkbox, module) {
  if (!checkbox || !module) return;
  module.classList.toggle('show', checkbox.checked);
  checkbox.closest('.switch-row')?.classList.toggle('active', checkbox.checked);
}

function syncFeatureModules() {
  toggleModule(toggleInterpolate, moduleInterpolate);
  toggleModule(toggleUpscale, moduleUpscale);
  toggleModule(toggleSloMo, moduleSloMo);
  toggleModule(toggleDecompress, moduleDecompress);
  toggleModule(toggleDenoise, moduleDenoise);
  toggleModule(toggleDeblur, moduleDeblur);
}

function spawnParticles(count = 24) {
  if (!particles) return;
  particles.innerHTML = '';
  for (let i = 0; i < count; i += 1) {
    const node = document.createElement('span');
    node.className = 'particle';
    const size = Math.random() * 2.8 + 1.2;
    node.style.width = `${size}px`;
    node.style.height = `${size}px`;
    node.style.left = `${Math.random() * 100}%`;
    node.style.top = `${Math.random() * 100}%`;
    node.style.setProperty('--spd', `${(Math.random() * 6 + 4).toFixed(2)}s`);
    node.style.animationDelay = `${(Math.random() * 4).toFixed(2)}s`;
    particles.appendChild(node);
  }
}

function animateKpis() {
  const gpuValue = latestRuntimeMetrics.gpu_util_percent;
  const hasGpuValue = gpuValue !== null && gpuValue !== undefined && Number.isFinite(Number(gpuValue));
  const fps = generationState.fps ?? 0;
  const healthy = backendConnectionStatus === 'offline'
    ? 'Offline'
    : backendConnectionStatus === 'checking'
      ? 'Connecting'
      : fps > 70 ? 'Optimal' : fps > 40 ? 'Stable' : generationState.status === 'Running' ? 'Load High' : 'Ready';
  if (gpuLoad) gpuLoad.textContent = hasGpuValue ? `${Math.round(Number(gpuValue))}%` : '--';
  if (gpuText) gpuText.textContent = hasGpuValue ? `GPU: ${Math.round(Number(gpuValue))}%` : 'GPU: telemetry unavailable';
  if (throughput) throughput.textContent = fps > 0 ? `${fps.toFixed(1)} Gen FPS` : '--';
  if (health) health.textContent = healthy;
  if (gpuMeter) gpuMeter.style.width = hasGpuValue ? `${Math.min(100, Math.max(0, Number(gpuValue)))}%` : '0%';
  if (fpsMeter) fpsMeter.style.width = `${Math.min(100, fps)}%`;
  updatePreviewHud();
  updatePreviewDetails();
}

function qualityPresetToCrf(preset) {
  const map = {
    'Low': 28,
    'Medium': 23,
    'High': 18,
    'Very High': 15,
    'Ultra': 12,
    'Lossless': 0,
  };
  return map[preset] ?? 18;
}

function updateEncoderPreview() {
  if (!encoderPreview) return;
  const codec = videoEncoder?.value || 'libx264';
  const speed = videoEncoderSpeed?.value || 'medium';
  const quality = videoCrf?.value || '18';
  const pixFmt = videoPixelFormat?.value || 'yuv420p';
  const audCodec = audioEncoder?.value || 'copy_audio';
  const audBr = audioBitrate?.value || '192k';
  const subCodec = subtitleEncoder?.value || 'copy_subtitle';
  const tune = videoTune?.value || 'none';
  const gop = Math.max(1, Math.min(300, Number(keyframeInterval?.value || 48)));
  const customOptions = customEncoder?.value.trim();
  const ext = videoContainer?.value || 'mkv';
  if (customOptions) {
    encoderPreview.value = `ffmpeg -i input.mp4 ${customOptions} output_enhanced.${ext}`;
    return;
  }
  const command = [
    `ffmpeg -i input.mp4 -c:v ${codec} -preset ${speed} -crf ${quality}${tune !== 'none' ? ` -tune ${tune}` : ''} -g ${gop}`,
    `-pix_fmt ${pixFmt} -c:a ${audCodec} -b:a ${audBr} -c:s ${subCodec}`,
    `output_enhanced.${ext}`,
  ].join(' ');
  encoderPreview.value = command;
}

function openPreviewOverlay() {
  if (!previewOverlay) return;
  previewOverlay.classList.add('open');
  previewOverlay.setAttribute('aria-hidden', 'false');
  document.body.style.overflow = 'hidden';
}

function closePreviewOverlayPanel() {
  if (!previewOverlay) return;
  previewOverlay.classList.remove('open');
  previewOverlay.setAttribute('aria-hidden', 'true');
  document.body.style.overflow = '';
}

async function fetchRuntimeMetrics() {
  if (runtimeMetricsFetchInFlight) return;
  runtimeMetricsFetchInFlight = true;
  try {
    const payload = await apiGet('/api/runtime_metrics');
    latestRuntimeMetrics = {
      ...latestRuntimeMetrics,
      ...payload,
    };
  } catch (_err) {
    // keep previous metrics if runtime endpoint is unavailable
  } finally {
    runtimeMetricsFetchInFlight = false;
    animateKpis();
  }
}

function startRuntimeMetricsPolling() {
  if (runtimeMetricsPollHandle !== null) window.clearInterval(runtimeMetricsPollHandle);
  fetchRuntimeMetrics();
  runtimeMetricsPollHandle = window.setInterval(fetchRuntimeMetrics, 2000);
}

videoQualityPreset?.addEventListener('change', () => {
  if (!videoCrf) return;
  videoCrf.value = String(qualityPresetToCrf(videoQualityPreset.value));
  updateEncoderPreview();
});

[videoEncoder, videoEncoderSpeed, videoCrf, videoContainer, videoPixelFormat, audioEncoder, audioBitrate, subtitleEncoder, customEncoder, videoTune, keyframeInterval]
  .forEach((el) => el?.addEventListener('change', () => {
    updateEncoderPreview();
    updatePreviewDetails();
  }));

[videoCrf]
  .forEach((el) => el?.addEventListener('input', () => {
    updateEncoderPreview();
    updatePreviewDetails();
  }));

function queueItems() {
  return [...queue.querySelectorAll('li')];
}

function renderSelection() {
  queueItems().forEach((li, i) => li.classList.toggle('selected', i === selectedIndex));
}

function addQueueItem(name, inputPath = '') {
  const existing = queueItems().find((item) => (item.dataset.inputPath || '') === String(inputPath || ''));
  if (existing) {
    selectedIndex = queueItems().indexOf(existing);
    renderSelection();
    return false;
  }
  const li = document.createElement('li');
  li.dataset.inputPath = inputPath || '';
  li.innerHTML = `<span>${name}</span><span>Queued</span>`;
  li.addEventListener('click', () => {
    selectedIndex = queueItems().indexOf(li);
    renderSelection();
  });
  queue.appendChild(li);
  selectedIndex = queueItems().length - 1;
  renderSelection();
  return true;
}

function setQueueItemStatus(item, status, tone = '') {
  if (!item) return;
  const statusNode = item.querySelector('span:last-child');
  if (statusNode) statusNode.textContent = status;
  item.dataset.status = status;
  item.classList.toggle('queue-running', tone === 'running');
  item.classList.toggle('queue-done', tone === 'done');
  item.classList.toggle('queue-error', tone === 'error');
}

document.getElementById('addQueue')?.addEventListener('click', async () => {
  const inputPath = inputPathField?.value || '';
  if (!inputPath || inputPath === 'No file selected') {
    showEtaToast('Select an input video first', true);
    return;
  }
  const name = basename(inputPath);
  const exists = queueItems().some((item) => (item.dataset.inputPath || '') === String(inputPath));
  if (exists) {
    addQueueItem(name, inputPath);
    log(`${name} is already in the render queue`);
    return;
  }
  try {
    await prepareModelsForQueue();
    addQueueItem(name, inputPath);
    log(`Added ${name} to render queue`);
  } catch (err) {
    showToastNotification('Model download failed', String(err), true);
    appendDownloadLog(`Could not queue ${name}: ${String(err)}`);
  }
});

document.getElementById('queueRemove')?.addEventListener('click', () => {
  const items = queueItems();
  if (selectedIndex < 0 || !items[selectedIndex]) return;
  log(`Removed ${items[selectedIndex].innerText.split('\n')[0]} from queue`);
  items[selectedIndex].remove();
  selectedIndex = Math.min(selectedIndex, queueItems().length - 1);
  renderSelection();
});

function moveSelected(dir) {
  const items = queueItems();
  if (selectedIndex < 0) return;
  const newIndex = selectedIndex + dir;
  if (newIndex < 0 || newIndex >= items.length) return;
  const item = items[selectedIndex];
  if (dir < 0) queue.insertBefore(item, items[newIndex]);
  else queue.insertBefore(items[newIndex], item);
  selectedIndex = newIndex;
  renderSelection();
}

document.getElementById('queueUp')?.addEventListener('click', () => moveSelected(-1));
document.getElementById('queueDown')?.addEventListener('click', () => moveSelected(1));

let lastRenderStatus = 'idle';
let lastRenderOutput = null;
let batchRenderActive = false;
let renderStopRequested = false;
let renderPaused = false;

async function fetchRenderStatus() {
  try {
    const status = await apiGet('/api/render/status');
    const state = String(status.status || 'idle').toLowerCase();
    const eta = status.eta || '--';
    const fps = status.gen_fps === null || status.gen_fps === undefined ? '--' : String(status.gen_fps);
    generationState.frame = Number(status.current_frame || 0);
    generationState.downloadActive = !!status.download_active;
    generationState.downloadProgress = Number(status.download_progress || 0);
    generationState.downloadLabel = String(status.download_label || '').trim();
    generationState.downloadDetail = String(status.download_detail || '').trim();
    const stateLabel = status.download_active || state === 'downloading_model'
      ? 'DOWNLOADING MODEL'
      : state.toUpperCase();
    setStatus(eta, fps, stateLabel);
    const pauseLabel = status.paused ? 'Resume' : 'Pause';
    const pauseMiniLabel = status.paused ? '>' : '||';
    renderPaused = !!status.paused;
    const pauseButton = document.getElementById('pauseRender');
    if (pauseButton) pauseButton.textContent = pauseLabel;
    if (previewPauseBtn) previewPauseBtn.textContent = pauseMiniLabel;
    setLiveModelDownloadToast(status);
    notifyEtaUpdate(eta, state);
    updatePreviewProgress();
    if (generationState.downloadActive) {
      setLivePreviewText(`Downloading ${generationState.downloadLabel || 'model'} · ${Math.round(generationState.downloadProgress)}%`);
    } else if (generationState.frame > 0) {
      clearPreviewLabel();
    }
    const previewStateFinished = ['completed', 'stopped', 'error'].includes(state) && lastRenderStatus !== state;
    if (!generationState.downloadActive && previewIsEnabled() && (status.preview?.available || previewStateFinished)) {
      refreshPreviewFrame(state);
    }
    if (Array.isArray(status.log_lines)) {
      if (status.log_lines.length < renderedBackendLogCount) {
        renderedBackendLogCount = 0;
      }
      if (status.log_lines.length > renderedBackendLogCount) {
        status.log_lines.slice(renderedBackendLogCount).forEach((line) => log(line));
        renderedBackendLogCount = status.log_lines.length;
        if (status.log_lines.length) {
          RENDER_LAST_LOG_CACHE = status.log_lines[status.log_lines.length - 1];
        }
      }
    } else if (status.last_log && status.last_log !== RENDER_LAST_LOG_CACHE) {
      RENDER_LAST_LOG_CACHE = status.last_log;
      log(status.last_log);
    }
    if (state === 'completed' && lastRenderStatus !== 'completed') {
      setLiveModelDownloadToast({ download_active: false });
      if (status.output_file && status.output_file !== lastRenderOutput) {
        const out = document.createElement('li');
        out.innerHTML = `<span>${basename(status.output_file)}</span><span>Done</span>`;
        outputFiles.prepend(out);
        lastRenderOutput = status.output_file;
      }
      setLiveEtaToast('--', 'completed');
      notifyRenderEvent(
        'Render',
        status.output_file ? `${basename(status.output_file)} finished` : 'Complete',
        { done: true, desktop: true, tag: 'render-complete' },
      );
      clearPreviewLabel();
    }
    if (state === 'error' && lastRenderStatus !== 'error') {
      setLiveModelDownloadToast({ download_active: false });
      setLiveEtaToast('--', 'error');
      const detail = String(status.error || status.log_lines?.slice(-8).join('\n') || 'Check the Logs tab for the renderer error.');
      showToastNotification('Render failed', detail, true);
      notifyRenderEvent('Render', detail, { done: true, desktop: true, tag: 'render-error' });
      setLivePreviewText(livePreviewImage?.getAttribute('src') ? '' : detail);
    }
    if (state === 'stopped' || state === 'idle') {
      setLiveModelDownloadToast({ download_active: false });
      setLiveEtaToast('--', state);
      if (state === 'stopped') {
        setLivePreviewText(livePreviewImage?.getAttribute('src') ? '' : 'Render stopped before the first preview frame');
      }
    }
    lastRenderStatus = state;
    return status;
  } catch (_err) {
    return null;
  }
}

let RENDER_LAST_LOG_CACHE = '';

function startRenderStatusPolling() {
  fetchRenderStatus();
  if (renderStatusPollHandle) window.clearInterval(renderStatusPollHandle);
  renderStatusPollHandle = window.setInterval(fetchRenderStatus, 1000);
}

function resolveOutputPathForInput(inputPath) {
  let outputPath = outputPathField?.value || '';
  if ((!outputPath || outputPath === 'No folder selected') && setUseSameOutputFolder?.checked) {
    const normalized = String(inputPath).replace(/\\/g, '/');
    const idx = normalized.lastIndexOf('/');
    outputPath = idx > 0 ? normalized.slice(0, idx).replace(/\//g, '\\') : '';
    if (outputPathField && outputPath) outputPathField.value = outputPath;
  }
  if ((!outputPath || outputPath === 'No folder selected') && !setUseSameOutputFolder?.checked) {
    outputPath = setOutputFolderLocation?.value || '';
    if (outputPathField && outputPath) outputPathField.value = outputPath;
  }
  return outputPath;
}

function buildRenderPayload(inputPath, outputPath) {
  const startValue = String(renderStartTime?.value || '').trim();
  const endValue = String(renderEndTime?.value || '').trim();
  const advancedSettings = readAdvancedSettingsFromUI();
  const payload = {
    engine: processingEngine?.value || 'rve',
    input_path: inputPath,
    output_folder: outputPath,
    ...(startValue ? { start_time: Number(startValue) } : {}),
    ...(endValue ? { end_time: Number(endValue) } : {}),
    backend: backendSelect?.value || 'auto',
    render_profile: document.getElementById('renderProfile')?.value || 'Balanced',
    interpolate_factor: Number(document.getElementById('interpolateFactor')?.value || 2),
    override_upscale_scale: Number(document.getElementById('upscaleScale')?.value || 0),
    denoise_strength: Math.min(10, Math.max(1, Number(document.getElementById('denoiseStrength')?.value || 5))),
    denoise_temporal: String(document.getElementById('denoiseTemporal')?.value || 'Off').toLowerCase(),
    denoise_keep_film_grain: !!document.getElementById('denoiseFilmGrain')?.checked,
    deblur_strength: Math.min(10, Math.max(1, Number(document.getElementById('deblurStrength')?.value || 6))),
    deblur_edge_recovery: Math.min(100, Math.max(0, Number(document.getElementById('deblurEdgeRecovery')?.value || 0))),
    deblur_ghosting_guard: String(document.getElementById('deblurGhostingGuard')?.value || 'Off').toLowerCase(),
    toggles: {
      interpolate: !!toggleInterpolate?.checked,
      upscale: !!toggleUpscale?.checked,
      decompress: !!toggleDecompress?.checked,
      denoise: !!toggleDenoise?.checked,
      deblur: !!toggleDeblur?.checked,
      slomo: !!toggleSloMo?.checked,
    },
    models: {
      interpolate: interpolateModel?.value || null,
      upscale: upscaleModel?.value || null,
      decompress: decompressModel?.value || null,
      denoise: denoiseModel?.value || null,
      deblur: deblurModel?.value || null,
    },
    settings: {
      ...advancedSettings,
      // The renderer enables tiling from the tile size; zero keeps the full-frame path.
      tile_size: advancedSettings.tiling_enabled ? advancedSettings.tile_size : 0,
      overlap: advancedSettings.tiling_enabled ? advancedSettings.overlap : 0,
    },
    animescripter: {
      smooth_dedup: !!document.getElementById('tasSmoothDedup')?.checked,
      depth: !!document.getElementById('tasDepth')?.checked,
      segment: !!document.getElementById('tasSegment')?.checked,
      stabilize: !!document.getElementById('tasStabilize')?.checked,
      depth_quality: document.getElementById('tasDepthQuality')?.value || 'low',
    },
    encoding: readEncodingSettingsFromUI(),
  };
  return payload;
}

function updateProcessingEngineUI() {
  const animeSelected = processingEngine?.value === 'animescripter';
  if (animeScripterOptions) animeScripterOptions.hidden = !animeSelected;
}

async function refreshAnimeScripterStatus() {
  if (!animeScripterStatus) return;
  try {
    const status = await apiGet('/api/animescripter/status');
    animeScripterStatus.textContent = status.available ? `AnimeScripter found · ${status.executable}` : status.detail;
    animeScripterStatus.dataset.state = status.available ? 'ready' : 'missing';
    if (installAnimeScripterBtn) installAnimeScripterBtn.hidden = Boolean(status.available);
    return status;
  } catch (_error) {
    animeScripterStatus.textContent = 'API connection required to detect AnimeScripter';
    animeScripterStatus.dataset.state = 'unknown';
    if (installAnimeScripterBtn) installAnimeScripterBtn.hidden = true;
    return null;
  }
}

async function installAnimeScripterFromUI() {
  if (!installAnimeScripterBtn) return;
  installAnimeScripterBtn.disabled = true;
  installAnimeScripterBtn.textContent = 'Installing…';
  animeScripterStatus.textContent = 'Installing the official AnimeScripter CLI into the launcher runtime…';
  animeScripterStatus.dataset.state = 'unknown';
  try {
    const initial = await apiPost('/api/animescripter/install', {});
    renderRuntimeInstallState(initial);
    while (true) {
      const state = await apiGet('/api/startup/install_status');
      renderRuntimeInstallState(state);
      if (String(state.status || '').toLowerCase() === 'completed') break;
      if (String(state.status || '').toLowerCase() === 'error') throw new Error(state.detail || 'AnimeScripter installation failed.');
      await sleep(900);
    }
    const status = await refreshAnimeScripterStatus();
    if (!status?.available) throw new Error(status?.detail || 'The installer completed, but AnimeScripter could not be detected.');
    if (animeScripterStatus) animeScripterStatus.textContent = 'AnimeScripter CLI installed and ready. Its model weights download when first used.';
    showToastNotification('AnimeScripter ready', 'The CLI is installed in the RVE launcher runtime.');
  } catch (error) {
    const rawError = String(error || 'Unknown installation error');
    const message = /urlopen error|winerror 10013|zugriff auf einen socket/i.test(rawError)
      ? 'Could not reach the AnimeScripter installer. Check network access, then retry.'
      : rawError;
    if (animeScripterStatus) {
      animeScripterStatus.textContent = `AnimeScripter install failed: ${message}`;
      animeScripterStatus.dataset.state = 'missing';
    }
    appendDownloadLog(`AnimeScripter install failed: ${message}${message === rawError ? '' : ` (${rawError})`}`);
  } finally {
    installAnimeScripterBtn.disabled = false;
    installAnimeScripterBtn.textContent = 'Install AnimeScripter';
  }
}

function waitForRenderToFinish() {
  return new Promise((resolve) => {
    const timer = window.setInterval(async () => {
      const status = await fetchRenderStatus();
      const state = String(status?.status || '').toLowerCase();
      if (['completed', 'error', 'stopped'].includes(state)) {
        window.clearInterval(timer);
        resolve(status);
      }
    }, 1000);
  });
}

async function startRenderWithOverwritePrompt(payload) {
  try {
    return await apiPost('/api/render/start', payload);
  } catch (error) {
    if (error?.code !== 'OUTPUT_EXISTS') throw error;
    const outputFile = error.outputFile || '';
    const confirmed = window.confirm(
      `This output file already exists:\n\n${outputFile}\n\nReplace it with the new render?`
    );
    if (!confirmed) return { ok: false, cancelled: true, error: 'Render cancelled; the existing output was kept.' };
    payload.settings = { ...(payload.settings || {}), overwrite: true };
    return apiPost('/api/render/start', payload);
  }
}

async function runQueuedBatchRender(items) {
  if (batchRenderActive) return;
  batchRenderActive = true;
  renderStopRequested = false;
  try {
    for (let index = 0; index < items.length; index += 1) {
      if (renderStopRequested) break;
      const item = items[index];
      const inputPath = item.dataset.inputPath || '';
      if (!inputPath) {
        setQueueItemStatus(item, 'Skipped', 'error');
        continue;
      }
      const outputPath = advOutputToMpv?.checked ? '' : resolveOutputPathForInput(inputPath);
      if ((!outputPath || outputPath === 'No folder selected') && !advOutputToMpv?.checked) {
        setQueueItemStatus(item, 'No output', 'error');
        showEtaToast('Select an output folder first', true);
        break;
      }
      if (inputPathField) inputPathField.value = inputPath;
      selectedIndex = index;
      renderSelection();
      setQueueItemStatus(item, `Running ${index + 1}/${items.length}`, 'running');
      log(`Batch render ${index + 1}/${items.length}: ${basename(inputPath)}`);
      const payload = buildRenderPayload(inputPath, outputPath);
      payload.queue_index = index + 1;
      payload.queue_total = items.length;
      let result = null;
      try {
        result = await startRenderWithOverwritePrompt(payload);
      } catch (err) {
        setQueueItemStatus(item, 'Failed', 'error');
        reportRenderStartError(err, 'Queued render');
        break;
      }
      if (!result.ok) {
        if (result.cancelled) {
          setQueueItemStatus(item, 'Skipped', 'error');
          break;
        }
        setQueueItemStatus(item, 'Failed', 'error');
        showEtaToast(result.error || 'Failed to start queued render', true);
        break;
      }
      startRenderStatusPolling();
      const finalStatus = await waitForRenderToFinish();
      const state = String(finalStatus?.status || '').toLowerCase();
      if (state === 'completed') {
        setQueueItemStatus(item, 'Done', 'done');
      } else if (state === 'stopped') {
        setQueueItemStatus(item, 'Stopped', 'error');
        break;
      } else {
        setQueueItemStatus(item, 'Failed', 'error');
        break;
      }
    }
  } finally {
    batchRenderActive = false;
  }
}

async function startBackendRender() {
  const startValue = String(renderStartTime?.value || '').trim();
  const endValue = String(renderEndTime?.value || '').trim();
  const invalidStart = startValue && (!Number.isFinite(Number(startValue)) || Number(startValue) < 0);
  const invalidEnd = endValue && (!Number.isFinite(Number(endValue)) || Number(endValue) < 0);
  if (invalidStart || invalidEnd || (startValue && endValue && Number(endValue) <= Number(startValue))) {
    showEtaToast('Enter non-negative times, with the end after the start', true);
    return;
  }
  const interpolateEnabled = !!toggleInterpolate?.checked;
  const interpolationFactor = Number(interpolateFactor?.value || 1);
  if (advMotionBlur?.checked && (!interpolateEnabled || interpolationFactor <= 1 || toggleSloMo?.checked)) {
    showEtaToast('Motion blur requires interpolation above 1x and cannot use SloMo mode', true);
    return;
  }
  if (advDeduplicate?.checked && (!interpolateEnabled || interpolationFactor <= 1)) {
    showEtaToast('Duplicate frame skipping requires interpolation above 1x', true);
    return;
  }
  const queuedItems = queueItems().filter((item) => item.dataset.inputPath);
  if (queuedItems.length > 1) {
    await runQueuedBatchRender(queuedItems);
    return;
  }
  const inputPath = queuedItems[0]?.dataset.inputPath || inputPathField?.value || '';
  const outputPath = advOutputToMpv?.checked ? '' : resolveOutputPathForInput(inputPath);
  if (!inputPath || inputPath === 'No file selected') {
    showEtaToast('Select an input video first', true);
    return;
  }
  if ((!outputPath || outputPath === 'No folder selected') && !advOutputToMpv?.checked) {
    showEtaToast('Select an output folder first', true);
    return;
  }
  const payload = buildRenderPayload(inputPath, outputPath);
  try {
    log(`Render request prepared: ${basename(inputPath)} -> ${outputPath}`);
    primeDesktopNotifications();
    notifyRenderEvent('Render', 'Start queried', { tag: 'render-start' });
    generationState.frame = 0;
    generationState.downloadActive = false;
    generationState.downloadProgress = 0;
    generationState.downloadLabel = '';
    generationState.downloadDetail = '';
    renderedBackendLogCount = 0;
    lastEtaToastValue = '--';
    lastEtaToastAt = 0;
    updatePreviewProgress();
    setStatus('--', '--', 'STARTING');
    const result = await startRenderWithOverwritePrompt(payload);
    if (result.cancelled) {
      setStatus('--', '--', 'IDLE');
      showEtaToast(result.error, true);
      return;
    }
    if (!result.ok) {
      showEtaToast(result.error || 'Failed to start render', true);
      return;
    }
    startRenderStatusPolling();
    log('Render request accepted by backend');
    // Keep the decoded input still visible while the renderer warms up models
    // and prepares its first processed frame. Dropping it here made the preview
    // look broken throughout long model initialization periods.
    if (!livePreviewImage?.getAttribute('src')) {
      void loadInputFirstFrame(inputPath);
    }
  } catch (err) {
    reportRenderStartError(err, 'Render start');
  }
}

async function pauseBackendRender() {
  try {
    const nextPaused = !renderPaused;
    const payload = await apiPost('/api/render/pause', { paused: nextPaused });
    const paused = !!payload?.paused;
    renderPaused = paused;
    const pauseButton = document.getElementById('pauseRender');
    if (pauseButton) pauseButton.textContent = paused ? 'Resume' : 'Pause';
    if (previewPauseBtn) previewPauseBtn.textContent = paused ? '>' : '||';
    setStatus(generationState.eta || '--', generationState.fps || '--', paused ? 'PAUSED' : 'RUNNING');
    showEtaToast(paused ? 'Paused' : 'Resumed', true);
  } catch (err) {
    showEtaToast(`Pause failed: ${String(err)}`, true);
  }
}

async function stopBackendRender() {
  renderStopRequested = true;
  try {
    await apiPost('/api/render/stop', {});
    generationState.frame = 0;
    setStatus('--', '--', 'STOPPED');
    setLiveEtaToast('--', 'stopped');
    updatePreviewProgress();
    clearPreviewLabel();
    showEtaToast('Stopped', true);
  } catch (err) {
    showEtaToast(`Stop failed: ${String(err)}`, true);
  }
}

async function quitApplication() {
  showEtaToast('Closing REAL Video Enhancer...', true);
  try {
    await apiPost('/api/app/quit', {});
  } catch (_err) {
  }
  try {
    window.open('', '_self');
  } catch (_err) {
  }
  try {
    window.close();
  } catch (_err) {
  }
  window.setTimeout(() => {
    if (!window.closed) {
      try {
        document.body.innerHTML = '<div style="min-height:100vh;display:grid;place-items:center;background:#111822;color:#eef5ff;font-family:Outfit,sans-serif;">REAL Video Enhancer has closed. You can close this tab now.</div>';
      } catch (_err) {
      }
      try {
        window.location.replace('about:blank');
      } catch (_err) {
      }
    }
  }, 180);
}

document.getElementById('startRender')?.addEventListener('click', startBackendRender);
document.getElementById('pauseRender')?.addEventListener('click', pauseBackendRender);
document.getElementById('killRender')?.addEventListener('click', stopBackendRender);
previewPauseBtn?.addEventListener('click', pauseBackendRender);
previewStopBtn?.addEventListener('click', stopBackendRender);
quitAppBtn?.addEventListener('click', quitApplication);

log('Web UI initialized with desktop-style layout');
setStatus('--', '--', 'Ready');
clearPreviewLabel();
startupState.loader = new StartupWireframeLoader(startupWireframeCanvas, startupWireframeSvg);
startupState.loader.start();
spawnParticles();
animateKpis();

document.addEventListener('mousemove', (event) => {
  if (!cursorGlow) return;
  cursorGlow.style.left = `${event.clientX}px`;
  cursorGlow.style.top = `${event.clientY}px`;
});

boostVisualsBtn?.addEventListener('click', () => {
  document.body.classList.toggle('visual-boost');
  const active = document.body.classList.contains('visual-boost');
  boostVisualsBtn.textContent = active ? 'Standard Visuals' : 'Boost Visuals';
  spawnParticles(active ? 48 : 24);
  log(active ? 'Visual boost enabled' : 'Visual boost disabled');
});

backendSelect?.addEventListener('change', () => {
  populateModelsForBackend();
});

processingEngine?.addEventListener('change', () => {
  updateProcessingEngineUI();
  refreshAnimeScripterStatus();
});

[interpolateModel, upscaleModel].forEach((node) => {
  node?.addEventListener('change', updatePreviewDetails);
});

[toggleInterpolate, toggleUpscale, toggleSloMo, toggleDecompress, toggleDenoise, toggleDeblur].forEach((toggle) => {
  toggle?.addEventListener('change', () => {
    syncFeatureModules();
    updatePreviewDetails();
    const name = toggle.closest('.switch-row')?.querySelector('span')?.textContent || 'Feature';
    const state = toggle.checked ? 'enabled' : 'disabled';
    showEtaToast(`${name} ${state}`, true);
    log(`${name} ${state}`);
  });
});

installButtonsAsDesktopBridge();
installSettingsAutosave();
installPreviewToggleSync();
setupInputOutputPickers();
updateProcessingEngineUI();
refreshAnimeScripterStatus();
syncFeatureModules();
if (videoCrf && videoQualityPreset) {
  videoCrf.value = String(qualityPresetToCrf(videoQualityPreset.value));
}
setVideoContainer?.addEventListener('change', () => {
  if (videoContainer) videoContainer.value = setVideoContainer.value;
  updateEncoderPreview();
  updatePreviewDetails();
});
setVideoPixelFormat?.addEventListener('change', () => {
  if (videoPixelFormat) videoPixelFormat.value = setVideoPixelFormat.value;
  updateEncoderPreview();
  updatePreviewDetails();
});
interpolateFactor?.addEventListener('input', updatePreviewDetails);
interpolateFactor?.addEventListener('change', updatePreviewDetails);
upscaleScale?.addEventListener('input', updatePreviewDetails);
upscaleScale?.addEventListener('change', updatePreviewDetails);
setDynamicScaledFlow?.addEventListener('change', syncAdvancedMirrors);
setEnsemble?.addEventListener('change', syncAdvancedMirrors);
setTensorRTOptLevel?.addEventListener('input', syncAdvancedMirrors);
setTensorRTOptLevel?.addEventListener('input', syncTensorRtSettingsMirrors);
setBackendTensorRTOptLevel?.addEventListener('input', applyTensorRtSettingsMirrorsToAdvanced);
advDynamicScaledFlowMirror?.addEventListener('change', () => {
  if (setDynamicScaledFlow) setDynamicScaledFlow.checked = !!advDynamicScaledFlowMirror.checked;
  syncAdvancedMirrors();
});
advEnsembleMirror?.addEventListener('change', () => {
  if (setEnsemble) setEnsemble.checked = !!advEnsembleMirror.checked;
  syncAdvancedMirrors();
});
advTensorRTOptMirror?.addEventListener('input', () => {
  if (setTensorRTOptLevel) setTensorRTOptLevel.value = advTensorRTOptMirror.value;
  syncAdvancedMirrors();
});
updateEncoderPreview();
updatePreviewDetails();
renderBackendManagerState({
  torch_versions: ['2.9.0', '2.8.0', '2.6.0'],
  pytorch_backends: [
    { value: 'cuda', label: 'CUDA' },
    { value: 'xpu', label: 'XPU' },
  ],
  installed_backends: [],
  directml_supported: false,
  tensorrt_supported: true,
}, latestSystemInfo);
startRuntimeInstallPolling();

async function runStartupSequence() {
  setStartupStage(8, 'Preparing startup scene...', 'Loading the web shell and interface visuals');
  await sleep(120);
  setStartupStage(18, 'Binding control surface...', 'Linking tabs, controls, preview and render actions');
  await sleep(140);
  setStartupStage(28, 'Calibrating render UI...', 'Preparing encoder options, preview metadata and queue state');
  updateEncoderPreview();
  updatePreviewDetails();
  await sleep(120);
  let bootstrap = await bootstrapFromBackend((progress, status, detail) => {
    setStartupStage(progress, status, detail);
  });
  if (bootstrap?.ok && bootstrap.payload?.startup?.needs_runtime_install) {
    log('Automatic runtime setup started from the startup screen.');
    try {
      await runStartupInstallSequence(bootstrap.payload.startup.install_state || null);
      bootstrap = await bootstrapFromBackend((progress, status, detail) => {
        setStartupStage(progress, status, detail);
      });
    } catch (err) {
      setStartupInstallVisible(true);
      setStartupStage(
        100,
        'Runtime setup failed',
        String(err || 'A runtime dependency could not be installed. Review the setup log below.'),
      );
      log(`Startup runtime setup failed: ${String(err)}`);
      return;
    }
  }
  setStartupStage(92, 'Starting live telemetry...', 'Enabling runtime metrics, ETA updates and render polling');
  startRuntimeMetricsPolling();
  startRenderStatusPolling();
  await sleep(180);
  if (bootstrap?.ok) {
    backendConnectionStatus = 'online';
    await finishStartupSequence('Backend ready', 'Live preview, metrics and backend controls are ready');
    if (!(bootstrap.payload?.system?.available_backends || []).length) showFirstRunSetupPrompt();
    else if (bootstrap.payload?.startup?.animescripter_available === false
      && !bootstrap.payload?.settings?.animescripter_setup_dismissed) {
      log('AnimeScripter is missing; starting its bundled launcher setup.');
      showToastNotification('Installing AnimeScripter', 'The launcher is setting up the official CLI in the background.');
      // Let the browser workspace finish starting while the optional engine
      // installs in the background. Network/setup failures must not strand the
      // user on the warmup screen.
      void installAnimeScripterFromUI();
    }
  } else {
    backendConnectionStatus = 'offline';
    await finishStartupSequence('Fallback mode ready', 'The interface is ready, but backend live data is limited');
  }
  animateKpis();
}

runStartupSequence();
window.addEventListener('beforeunload', () => {
  startupState.loader?.stop();
  stopRuntimeInstallPolling();
  if (runtimeMetricsPollHandle !== null) window.clearInterval(runtimeMetricsPollHandle);
  if (renderStatusPollHandle !== null) window.clearInterval(renderStatusPollHandle);
  if (firstFrameObjectUrl) URL.revokeObjectURL(firstFrameObjectUrl);
  if (previewFrameObjectUrl) URL.revokeObjectURL(previewFrameObjectUrl);
});

livePreviewBox?.addEventListener('click', openPreviewOverlay);
livePreviewBox?.addEventListener('keydown', (event) => {
  if (event.key === 'Enter' || event.key === ' ') {
    event.preventDefault();
    openPreviewOverlay();
  }
});
closePreviewOverlay?.addEventListener('click', closePreviewOverlayPanel);
previewOverlayBg?.addEventListener('click', closePreviewOverlayPanel);
document.addEventListener('keydown', (event) => {
  if (event.key === 'Escape' && previewOverlay?.classList.contains('open')) {
    closePreviewOverlayPanel();
  }
});
