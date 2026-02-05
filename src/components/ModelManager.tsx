import { useModels } from "../hooks/useModels";

export default function ModelManager() {
  const { models, downloadProgress, downloadModel, deleteModel, isDownloading } = useModels();

  // Separate CoreML acceleration models from regular models
  const sttModels = models.filter((m) => m.model_type === "stt" && !m.id.includes(".mlmodelc"));
  const coremlModels = models.filter((m) => m.model_type === "stt" && m.id.includes(".mlmodelc"));
  const ttsModels = models.filter((m) => m.model_type === "tts");

  return (
    <div className="space-y-6">
      {/* STT Models */}
      <section>
        <h2 className="text-lg font-semibold text-slate-200 mb-3">Speech-to-Text Models</h2>
        <div className="space-y-2">
          {sttModels.map((model) => (
            <ModelCard
              key={model.id}
              model={model}
              progress={downloadProgress[model.id]}
              onDownload={() => downloadModel(model.id)}
              onDelete={() => deleteModel(model.id)}
              isDownloading={isDownloading(model.id)}
            />
          ))}
        </div>
      </section>

      {/* CoreML Acceleration Models */}
      {coremlModels.length > 0 && (
        <section>
          <h2 className="text-lg font-semibold text-slate-200 mb-1">Apple Silicon Acceleration</h2>
          <p className="text-xs text-slate-400 mb-3">
            Optional CoreML encoders that use the Neural Engine for faster transcription on Apple Silicon Macs.
            Download the encoder matching your Whisper model for best performance.
          </p>
          <div className="space-y-2">
            {coremlModels.map((model) => (
              <ModelCard
                key={model.id}
                model={model}
                progress={downloadProgress[model.id]}
                onDownload={() => downloadModel(model.id)}
                onDelete={() => deleteModel(model.id)}
                isDownloading={isDownloading(model.id)}
                isCoreML={true}
              />
            ))}
          </div>
        </section>
      )}

      {/* TTS Models */}
      <section>
        <h2 className="text-lg font-semibold text-slate-200 mb-3">Text-to-Speech Models</h2>
        <div className="space-y-2">
          {ttsModels.map((model) => (
            <ModelCard
              key={model.id}
              model={model}
              progress={downloadProgress[model.id]}
              onDownload={() => downloadModel(model.id)}
              onDelete={() => deleteModel(model.id)}
              isDownloading={isDownloading(model.id)}
            />
          ))}
        </div>
      </section>

      {/* Storage Info */}
      <div className="bg-slate-800/50 rounded-lg p-4">
        <h3 className="text-sm font-medium text-slate-300 mb-2">Storage</h3>
        <p className="text-xs text-slate-400">
          Models are stored in ~/Library/Application Support/com.blahcubed.app/models/
        </p>
      </div>
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
  isDownloading: boolean;
  isCoreML?: boolean;
}

function ModelCard({ model, progress, onDownload, onDelete, isDownloading, isCoreML }: ModelCardProps) {
  const isDownloaded = model.status === "downloaded";

  return (
    <div className={`rounded-lg p-4 ${isCoreML ? "bg-purple-900/20 border border-purple-800/30" : "bg-slate-800"}`}>
      <div className="flex items-start justify-between">
        <div className="flex-1">
          <div className="flex items-center space-x-2">
            <h3 className="font-medium text-slate-100">{model.name}</h3>
            {isCoreML && (
              <span className="px-2 py-0.5 text-xs bg-purple-500/20 text-purple-400 rounded-full">
                Neural Engine
              </span>
            )}
            {isDownloaded && (
              <span className="px-2 py-0.5 text-xs bg-green-500/20 text-green-400 rounded-full">
                Downloaded
              </span>
            )}
          </div>
          <p className="text-sm text-slate-400 mt-1">{model.description}</p>
          <p className="text-xs text-slate-500 mt-1">{model.size_display}</p>
        </div>

        <div className="ml-4">
          {isDownloading ? (
            <div className="w-20">
              <div className="h-2 bg-slate-700 rounded-full overflow-hidden">
                <div
                  className="h-full bg-sky-500 transition-all"
                  style={{ width: `${progress || 0}%` }}
                />
              </div>
              <p className="text-xs text-slate-400 text-center mt-1">{progress || 0}%</p>
            </div>
          ) : isDownloaded ? (
            <button
              onClick={onDelete}
              className="px-3 py-1.5 text-sm bg-red-500/10 text-red-400 rounded hover:bg-red-500/20 transition-colors"
            >
              Delete
            </button>
          ) : (
            <button
              onClick={onDownload}
              className="px-3 py-1.5 text-sm bg-sky-500 text-white rounded hover:bg-sky-600 transition-colors"
            >
              Download
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
