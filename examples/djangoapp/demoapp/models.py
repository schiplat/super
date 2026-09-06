from __future__ import annotations

import uuid

from django.db import models


class WorkItem(models.Model):
    """Demo unit of async work processed by Celery workers under Super."""

    class Status(models.TextChoices):
        PENDING = "pending", "Pending"
        RUNNING = "running", "Running"
        DONE = "done", "Done"
        FAILED = "failed", "Failed"

    id = models.UUIDField(primary_key=True, default=uuid.uuid4, editable=False)
    created_at = models.DateTimeField(auto_now_add=True)
    updated_at = models.DateTimeField(auto_now=True)
    status = models.CharField(
        max_length=16,
        choices=Status.choices,
        default=Status.PENDING,
        db_index=True,
    )
    seconds = models.FloatField(default=2.0)
    result = models.TextField(blank=True, default="")
    error = models.TextField(blank=True, default="")

    class Meta:
        ordering = ("-created_at",)

    def __str__(self) -> str:
        return f"WorkItem({self.id}, {self.status})"
