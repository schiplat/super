import { defineStore } from 'pinia';
import { useStorage } from '@vueuse/core';

export const useSettingsStore = defineStore('settings', () => {
  // Persisted via useStorage
  // Arg 1: localStorage key
  // Arg 2: default (15)
  const defaultPageSize = useStorage<number>('super-pref-page-size-v2', 15);

  return {
    defaultPageSize,
  };
});
