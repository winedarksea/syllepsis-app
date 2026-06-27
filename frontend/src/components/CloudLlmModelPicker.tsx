import { useEffect, useMemo, useState } from 'react';
import { api } from '../lib/api';
import type { CloudLlmModel } from '../types';

const CUSTOM_MODEL_VALUE = '__custom_model__';

interface CloudLlmModelPickerProps {
  provider: string;
  value: string;
  onChange: (model: string) => void;
  disabled?: boolean;
  selectClassName?: string;
  inputClassName?: string;
  modelPlaceholder?: string;
}

export function CloudLlmModelPicker({
  provider,
  value,
  onChange,
  disabled = false,
  selectClassName,
  inputClassName,
  modelPlaceholder = 'Model name',
}: CloudLlmModelPickerProps) {
  const [models, setModels] = useState<CloudLlmModel[]>([]);
  const [loading, setLoading] = useState(Boolean(provider));
  const [manualCustomMode, setManualCustomMode] = useState(false);

  useEffect(() => {
    if (!provider) return;
    let cancelled = false;
    api.listCloudLlmProviderModels(provider)
      .then((nextModels) => {
        if (!cancelled) setModels(nextModels);
      })
      .catch(() => {
        if (!cancelled) {
          setModels([]);
          setManualCustomMode(true);
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => { cancelled = true; };
  }, [provider]);

  const modelIds = useMemo(() => models.map((model) => model.id), [models]);
  const valueMatchesListedModel = value !== '' && modelIds.includes(value);
  const customMode = manualCustomMode
    || (value !== '' && !valueMatchesListedModel)
    || (!loading && modelIds.length === 0);

  return (
    <div className="cloud-model-picker">
      <select
        className={selectClassName}
        value={customMode ? CUSTOM_MODEL_VALUE : value}
        onChange={(event) => {
          const next = event.target.value;
          if (next === CUSTOM_MODEL_VALUE) {
            setManualCustomMode(true);
            return;
          }
          setManualCustomMode(false);
          onChange(next);
        }}
        disabled={disabled || loading}
      >
        <option value="">Select model</option>
        {modelIds.map((modelId) => (
          <option key={modelId} value={modelId}>{modelId}</option>
        ))}
        <option value={CUSTOM_MODEL_VALUE}>Custom model...</option>
      </select>
      {customMode && (
        <input
          className={inputClassName}
          value={value}
          onChange={(event) => onChange(event.target.value)}
          placeholder={modelPlaceholder}
          disabled={disabled}
        />
      )}
    </div>
  );
}
