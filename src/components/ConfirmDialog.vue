<script setup lang="ts">
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { confirmState, resolveConfirm } from "@/lib/confirm";

function onOpenChange(open: boolean) {
  // 点击遮罩/ESC 关闭时按「取消」处理
  if (!open) resolveConfirm(false);
}
</script>

<template>
  <Dialog :open="confirmState.open" @update:open="onOpenChange">
    <DialogContent class="sm:max-w-sm">
      <DialogHeader>
        <DialogTitle>{{ confirmState.title }}</DialogTitle>
        <DialogDescription>{{ confirmState.message }}</DialogDescription>
      </DialogHeader>
      <DialogFooter>
        <Button variant="outline" @click="resolveConfirm(false)">取消</Button>
        <Button
          :variant="confirmState.destructive ? 'destructive' : 'default'"
          @click="resolveConfirm(true)"
        >
          {{ confirmState.confirmText }}
        </Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>
