from django.urls import path

from demoapp import views

urlpatterns = [
    path("", views.home),
    path("health/", views.health),
    path("api/work/enqueue/", views.enqueue_work),
    path("api/work/<uuid:work_id>/", views.work_status),
]
