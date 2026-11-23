import { defineStore } from "pinia";
import { reactive } from "vue";

type Kind = "confirm" | "alert" | "prompt";

interface DialogState {
  visible: boolean;
  kind: Kind;
  title: string;
  message: string;
  input: string;
  danger: boolean;
  resolve: ((v: boolean | string | null) => void) | null;
}

export const useDialogs = defineStore("dialogs", () => {
  const state = reactive<DialogState>({
    visible: false,
    kind: "confirm",
    title: "",
    message: "",
    input: "",
    danger: false,
    resolve: null,
  });

  function open(kind: Kind, message: string, opts?: { title?: string; danger?: boolean; input?: string }) {
    state.kind = kind;
    state.message = message;
    state.title = opts?.title ?? (kind === "confirm" ? "请确认" : kind === "prompt" ? "请输入" : "提示");
    state.danger = opts?.danger ?? false;
    state.input = opts?.input ?? "";
    state.visible = true;
    return new Promise<boolean | string | null>((resolve) => {
      state.resolve = resolve;
    });
  }

  function done(value: boolean | string | null) {
    state.visible = false;
    const r = state.resolve;
    state.resolve = null;
    r?.(value);
  }

  return {
    state,
    confirm: (message: string, opts?: { title?: string; danger?: boolean }) =>
      open("confirm", message, opts) as Promise<boolean>,
    alert: (message: string, title?: string) => open("alert", message, { title }).then(() => undefined),
    prompt: (message: string, input?: string, title?: string) =>
      open("prompt", message, { title, input }) as Promise<string | null>,
    accept: () => done(state.kind === "prompt" ? state.input : true),
    cancel: () => done(state.kind === "prompt" ? null : false),
  };
});