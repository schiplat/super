from __future__ import annotations

from django.core.management.base import BaseCommand

from demoapp.models import WorkItem
from demoapp.tasks import process_work_item


class Command(BaseCommand):
    help = "Create a WorkItem and enqueue demoapp.process_work_item (Celery)."

    def add_arguments(self, parser):
        parser.add_argument("--seconds", type=float, default=1.0)

    def handle(self, *args, **options):
        seconds = max(0.0, float(options["seconds"]))
        item = WorkItem.objects.create(seconds=seconds)
        result = process_work_item.delay(str(item.id), seconds)
        self.stdout.write(
            self.style.SUCCESS(
                f"enqueued work_id={item.id} task_id={result.id} seconds={seconds}"
            )
        )
