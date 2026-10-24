import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface Model {
  id: string;
  name: string;
  model_type: string;
  size_bytes: number;
  size_display: string;
  download_url: string;
  status: string;
  description: string;
}

export function useModels() {
  const [models, setModels] = useState<Model[]>([]);
  const [downloadProgress, setDownloadProgress] = useState<Record<string, number>>({});
  const [downloading, setDownloading] = useState<Set<string>>(new Set());
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [activeSttModel, setActiveSttModel] = useState<string | null>(null);

  const setModelError = useCallback((modelId: string, message: string | null) => {
    setErrors((prev) => {
      const next = { ...prev };
      if (message) next[modelId] = message;
      else delete next[modelId];
      return next;
    });
  }, []);

  const loadActiveSttModel = useCallback(async () => {
    try {
      const s = await invoke<{ stt_model: string }>("get_settings");
      setActiveSttModel(s.stt_model);
    } catch (err) {
      console.error("Failed to load active STT model:", err);
    }
  }, []);

  useEffect(() => {
    loadModels();
    loadActiveSttModel();

    // Listen for download progress events
    const unlisten = listen<[string, { percentage: number }]>(
      "model-download-progress",
      (event) => {
        const [modelId, progress] = event.payload;
        setDownloadProgress((prev) => ({
          ...prev,
          [modelId]: progress.percentage,
        }));
      }
    );

    return () => {
      unlisten.then((fn) => fn());
    };
  }, [loadActiveSttModel]);

  const loadModels = async () => {
    try {
      const result = await invoke<Model[]>("list_models");
      setModels(result);
    } catch (err) {
      console.error("Failed to load models:", err);
    }
  };

  const downloadModel = useCallback(async (modelId: string) => {
    try {
      setDownloading((prev) => new Set(prev).add(modelId));
      setDownloadProgress((prev) => ({ ...prev, [modelId]: 0 }));
      setModelError(modelId, null);

      await invoke("download_model", { modelId });

      // Refresh models list
      await loadModels();
    } catch (err) {
      console.error("Failed to download model:", err);
      setModelError(modelId, `Download failed: ${String(err)}`);
    } finally {
      setDownloading((prev) => {
        const next = new Set(prev);
        next.delete(modelId);
        return next;
      });
      setDownloadProgress((prev) => {
        const next = { ...prev };
        delete next[modelId];
        return next;
      });
    }
  }, [setModelError]);

  const deleteModel = useCallback(async (modelId: string) => {
    try {
      setModelError(modelId, null);
      await invoke("delete_model", { modelId });
      await loadModels();
    } catch (err) {
      console.error("Failed to delete model:", err);
      setModelError(modelId, `Delete failed: ${String(err)}`);
    }
  }, [setModelError]);

  /** Make a downloaded STT model the one dictation uses (persisted in settings). */
  const selectSttModel = useCallback(async (modelId: string) => {
    try {
      setModelError(modelId, null);
      const current = await invoke<Record<string, unknown>>("get_settings");
      await invoke("update_settings", { settings: { ...current, stt_model: modelId } });
      setActiveSttModel(modelId);
    } catch (err) {
      console.error("Failed to set STT model:", err);
      setModelError(modelId, `Could not switch model: ${String(err)}`);
    }
  }, [setModelError]);

  const isDownloading = useCallback(
    (modelId: string) => downloading.has(modelId),
    [downloading]
  );

  return {
    models,
    downloadProgress,
    downloadModel,
    deleteModel,
    selectSttModel,
    activeSttModel,
    errors,
    isDownloading,
    refreshModels: loadModels,
  };
}
