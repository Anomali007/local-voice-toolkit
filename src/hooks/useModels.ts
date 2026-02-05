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

  useEffect(() => {
    loadModels();

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
  }, []);

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

      await invoke("download_model", { modelId });

      // Refresh models list
      await loadModels();
    } catch (err) {
      console.error("Failed to download model:", err);
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
  }, []);

  const deleteModel = useCallback(async (modelId: string) => {
    try {
      await invoke("delete_model", { modelId });
      await loadModels();
    } catch (err) {
      console.error("Failed to delete model:", err);
    }
  }, []);

  const isDownloading = useCallback(
    (modelId: string) => downloading.has(modelId),
    [downloading]
  );

  return {
    models,
    downloadProgress,
    downloadModel,
    deleteModel,
    isDownloading,
    refreshModels: loadModels,
  };
}
