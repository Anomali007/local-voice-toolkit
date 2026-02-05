import { useModels } from "../hooks/useModels";

export default function ModelManager() {
  const { models, downloadProgress, downloadModel, deleteModel, isDownloading } = useModels();

  const sttModels = models.filter((m) => m.model_type === "stt");
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
}

function ModelCard({ model, progress, onDownload, onDelete, isDownloading }: ModelCardProps) {
  const isDownloaded = model.status === "downloaded";

  return (
    <div className="bg-slate-800 rounded-lg p-4">
      <div className="flex items-start justify-between">
        <div className="flex-1">
          <div className="flex items-center space-x-2">
            <h3 className="font-medium text-slate-100">{model.name}</h3>
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
