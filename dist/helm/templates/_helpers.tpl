{{- define "oci-registry.name" -}}
	{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{- define "oci-registry.chart" -}}
	{{- .Chart.Name -}}
{{- end -}}

{{/*
Create a default fully qualified app name.
We truncate at 63 chars because some Kubernetes name fields are limited to this (by the DNS naming spec).
If release name contains chart name it will be used as a full name.
*/}}
{{- define "oci-registry.fullname" -}}
	{{- if .Values.fullnameOverride -}}
		{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" -}}
	{{- else -}}
		{{- $name := default .Chart.Name .Values.nameOverride -}}
		{{- if (contains $name .Release.Name) -}}
			{{- .Release.Name | trunc 63 | trimSuffix "-" -}}
		{{- else -}}
			{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" -}}
		{{- end -}}
	{{- end -}}
{{- end -}}

{{- define "oci-registry.image" -}}
	{{- with .Values.image -}}
		{{- printf "%s/%s:%s" .registry .name (.tag | toString) -}}
	{{- end -}}
{{- end -}}

{{- define "oci-registry.archiver_image" -}}
	{{- with .Values.archiver.image -}}
		{{- printf "%s/%s:%s" .registry .name (.tag | toString) -}}
	{{- end -}}
{{- end -}}

{{- define "oci-registry.labels" -}}
app.kubernetes.io/name: {{ template "oci-registry.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
helm.sh/chart: {{ template "oci-registry.chart" . }}
{{- if .Values.extraLabels -}}
{{- toYaml .Values.extraLabels -}}
{{- end -}}
{{- end -}}

{{- define "oci-registry.upstream_secret_name" -}}
	{{- default (printf "%s-%s" (include "oci-registry.fullname" .) "upstream") .Values.registry.upstream.auth_secret.name_override | quote }}
{{- end -}}

{{- define "oci-registry.s3_secret_name" -}}
	{{- default (printf "%s-%s" (include "oci-registry.fullname" .) "s3") .Values.registry.storage.s3.auth_secret.name_override | quote }}
{{- end -}}

