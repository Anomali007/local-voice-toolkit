import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

interface PermissionStatus {
  microphone: boolean;
  accessibility: boolean;
}

export function usePermissions(pollIntervalMs = 2000) {
  const [permissions, setPermissions] = useState<PermissionStatus | null>(null);

  const checkPermissions = useCallback(async () => {
    try {
      const status = await invoke<PermissionStatus>("check_permissions");
      setPermissions(status);
    } catch (err) {
      console.error("Failed to check permissions:", err);
    }
  }, []);

  useEffect(() => {
    checkPermissions();

    const interval = setInterval(checkPermissions, pollIntervalMs);
    return () => clearInterval(interval);
  }, [checkPermissions, pollIntervalMs]);

  return permissions;
}
