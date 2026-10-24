import { useState } from "react";
import { useModels } from "../hooks/useModels";
import { PageHeader, Section } from "./ui";

export default function ModelManager() {
  const {
    models,
    downloadProgress,
    downloadModel,
    deleteModel,
    selectSttModel,
    activeSttModel,
    errors,
    isDownloading,
  } = useModels();
  const [confirmingDelete, setConfirmingDelete] = useState<string | null>(null);

  // Separate CoreML acceleration models from regular models
  const sttModels = models.filter((m) => m.model_type === "stt" && !m.id.includes(".mlmodelc"));
  const coremlModels = models.filter((m) => m.model_type === "stt" && m.id.includes(".mlmodelc"));
  const ttsModels = models.filter((m) => m.model_type === "tts");

  return (
    <div className="space-y-6">
      <PageHeader title="Speech models" lede="Everything runs on this Mac. Download a model once and it works offline." />

      {/* STT Models */}
      <Section title="Dictation models" note="Pick the model that transcribes your speech. Larger models are more accurate but slower.">
        <div className="card card-list overflow-hidden">
          {sttModels.map((model) => (
            <ModelCard
              key={model.id}
              model={model}
              progress={downloadProgress[model.id]}
              onDownload={() => downloadModel(model.id)}
              onDelete={() => {
                setConfirmingDelete(null);
                deleteModel(model.id);
              }}
              isDownloading={isDownloading(model.id)}
              error={errors[model.id]}
              confirming={confirmingDelete === model.id}
              onRequestDelete={() => setConfirmingDelete(model.id)}
              onCancelDelete={() => setConfirmingDelete(null)}
              isActive={model.id === activeSttModel}
              onUse={() => selectSttModel(model.id)}
              selectable
            />
          ))}
        </div>
      </Section>

      {/* CoreML Acceleration Models */}
      {coremlModels.length > 0 && (
        <Section title="Apple Silicon acceleration" note="Optional encoders that use the Neural Engine for faster transcription. Download the one that matches your dictation model.">
          <div className="card card-list overflow-hidden">
            {coremlModels.map((model) => (
              <ModelCard
                key={model.id}
                model={model}
                progress={downloadProgress[model.id]}
                onDownload={() => downloadModel(model.id)}
                onDelete={() => {
                setConfirmingDelete(null);
                deleteModel(model.id);
              }}
                isDownloading={isDownloading(model.id)}
                error={errors[model.id]}
                confirming={confirmingDelete === model.id}
                onRequestDelete={() => setConfirmingDelete(model.id)}
                onCancelDelete={() => setConfirmingDelete(null)}
                isCoreML={true}
              />
            ))}
          </div>
        </Section>
      )}

      {/* TTS Models */}
      <Section title="Reader voices">
        <div className="card card-list overflow-hidden">
          {ttsModels.map((model) => (
            <ModelCard
              key={model.id}
              model={model}
              progress={downloadProgress[model.id]}
              onDownload={() => downloadModel(model.id)}
              onDelete={() => {
                setConfirmingDelete(null);
                deleteModel(model.id);
              }}
              isDownloading={isDownloading(model.id)}
              error={errors[model.id]}
              confirming={confirmingDelete === model.id}
              onRequestDelete={() => setConfirmingDelete(model.id)}
              onCancelDelete={() => setConfirmingDelete(null)}
            />
          ))}
        </div>
      </Section>

      {/* Storage Info */}
      <p className="selectable px-1 text-xs text-subtle">
        Stored in ~/Library/Application Support/com.blahcubed.app/models/
      </p>
    </div>
  );
}

interface Model {
  id: string;
  name: string;
  model_type: string;
  size_display: string;
  description: string;
  status: string;
}


interface ModelCardProps {
  model: Model;
  progress?: number;
  onDownload: () => void;
  onDelete: () => void;
  onRequestDelete: () => void;
  onCancelDelete: () => void;
  confirming: boolean;
  error?: string;
  isDownloading: boolean;
  isCoreML?: boolean;
  /** STT model that can be chosen for dictation */
  selectable?: boolean;
  isActive?: boolean;
  onUse?: () => void;
}

function ModelCard({
  model,
  progress,
  onDownload,
  onDelete,
  onRequestDelete,
  onCancelDelete,
  confirming,
  error,
  isDownloading,
  isCoreML,
  selectable,
  isActive,
  onUse,
}: ModelCardProps) {
  const isDownloaded = model.status === "downloaded";

  return (
    <div className={`px-4 py-3.5 ${isActive ? "bg-accent/[0.07]" : ""}`}>
      <div className="flex items-center justify-between gap-4">
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-1.5">
            <h3 className="text-[13px] font-semibold">{model.name}</h3>
            {isCoreML && <span className="badge-neutral">Neural Engine</span>}
            {isDownloaded && !isActive && <span className="badge-ok">Downloaded</span>}
            {isActive && <span className="badge-accent">Active</span>}
          </div>
          <p className="mt-0.5 text-xs text-muted">{model.description}</p>
          <p className="mt-0.5 text-xs text-subtle">{model.size_display}</p>
        </div>

        <div className="flex shrink-0 items-center gap-2">
          {isDownloading ? (
            <div className="flex w-28 items-center gap-2" role="progressbar" aria-valuenow={progress || 0} aria-valuemin={0} aria-valuemax={100} aria-label={`Downloading ${model.name}`}>
              <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-line">
                <div className="h-full rounded-full bg-accent transition-all" style={{ width: `${progress || 0}%` }} />
              </div>
              <span className="w-8 text-right text-xs tabular-nums text-muted">{progress || 0}%</span>
            </div>
          ) : isDownloaded ? (
            <>
              {selectable && !isActive && (
                <button onClick={onUse} className="btn-primary">
                  Use this model
                </button>
              )}
              {!confirming && (
                <button onClick={onRequestDelete} className="btn-danger">
                  Delete
                </button>
              )}
            </>
          ) : (
            <button onClick={onDownload} className="btn-primary">
              {error ? "Retry" : "Download"}
            </button>
          )}
        </div>
      </div>

      {confirming && isDownloaded && (
        <div role="alertdialog" aria-label={`Delete ${model.name}`} className="mt-3 rounded-lg bg-sunken p-3">
          <p className="text-[13px]">Delete {model.name}? You can download it again later.</p>
          {isActive && (
            <p className="mt-1 text-xs text-warn">
              This is your active dictation model. Dictation will fail until you download it again or
              choose another model.
            </p>
          )}
          <div className="mt-2 flex gap-2">
            <button onClick={onDelete} className="btn-danger-solid">
              Confirm delete
            </button>
            <button onClick={onCancelDelete} className="btn-secondary">
              Cancel
            </button>
          </div>
        </div>
      )}

      {error && (
        <p role="alert" className="mt-2 break-words text-xs text-danger">
          {error}
        </p>
      )}
    </div>
  );
}
