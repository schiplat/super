from __future__ import annotations

from django.core.management.base import BaseCommand
from django.utils import timezone

from demoapp.tasks import ping


class Command(BaseCommand):
    help = "Super-cron friendly tick: log time and fire demoapp.ping."

    def handle(self, *args, **options):
        now = timezone.now().isoformat()
        async_result = ping.delay()
        self.stdout.write(f"demo_tick at {now} ping_task={async_result.id}")
