<?xml version='1.0' encoding='UTF-8'?>
<Project Type="Project" LVVersion="20008000">
	<Item Name="My Computer" Type="My Computer">
		<Property Name="server.app.propertiesEnabled" Type="Bool">true</Property>
		<Property Name="server.control.propertiesEnabled" Type="Bool">true</Property>
		<Property Name="server.tcp.enabled" Type="Bool">false</Property>
		<Property Name="server.tcp.port" Type="Int">0</Property>
		<Property Name="server.tcp.serviceName" Type="Str">My Computer/VI Server</Property>
		<Property Name="server.tcp.serviceName.default" Type="Str">My Computer/VI Server</Property>
		<Property Name="server.vi.callsEnabled" Type="Bool">true</Property>
		<Property Name="server.vi.propertiesEnabled" Type="Bool">true</Property>
		<Property Name="specify.custom.address" Type="Bool">false</Property>
		<Item Name="Asset.lvclass" Type="LVClass" URL="../client/Asset/Asset.lvclass"/>
		<Item Name="AssetAttachDatasetStaging.lvclass" Type="LVClass" URL="../client/AssetAttachDatasetStaging/AssetAttachDatasetStaging.lvclass"/>
		<Item Name="AssetCreateStaging.lvclass" Type="LVClass" URL="../client/AssetCreateStaging/AssetCreateStaging.lvclass"/>
		<Item Name="AssetUpdateStaging.lvclass" Type="LVClass" URL="../client/AssetUpdateStaging/AssetUpdateStaging.lvclass"/>
		<Item Name="Channel.lvclass" Type="LVClass" URL="../client/Channel/Channel.lvclass"/>
		<Item Name="Client.lvclass" Type="LVClass" URL="../client/Client/Client.lvclass"/>
		<Item Name="DataflashIngestStaging.lvclass" Type="LVClass" URL="../client/DataflashIngestStaging/DataflashIngestStaging.lvclass"/>
		<Item Name="Dataset.lvclass" Type="LVClass" URL="../client/Dataset/Dataset.lvclass"/>
		<Item Name="DatasetCreateStaging.lvclass" Type="LVClass" URL="../client/DatasetCreateStaging/DatasetCreateStaging.lvclass"/>
		<Item Name="DatasetUpdateStaging.lvclass" Type="LVClass" URL="../client/DatasetUpdateStaging/DatasetUpdateStaging.lvclass"/>
		<Item Name="IngestJob.lvclass" Type="LVClass" URL="../client/IngestJob/IngestJob.lvclass"/>
		<Item Name="List.lvclass" Type="LVClass" URL="../client/List/List.lvclass"/>
		<Item Name="McapIngestStaging.lvclass" Type="LVClass" URL="../client/McapIngestStaging/McapIngestStaging.lvclass"/>
		<Item Name="Run.lvclass" Type="LVClass" URL="../client/Run/Run.lvclass"/>
		<Item Name="RunCreateStaging.lvclass" Type="LVClass" URL="../client/RunCreateStaging/RunCreateStaging.lvclass"/>
		<Item Name="RunUpdateStaging.lvclass" Type="LVClass" URL="../client/RunUpdateStaging/RunUpdateStaging.lvclass"/>
		<Item Name="TabularIngestStaging.lvclass" Type="LVClass" URL="../client/TabularIngestStaging/TabularIngestStaging.lvclass"/>
		<Item Name="Template.lvclass" Type="LVClass" URL="../client/Template/Template.lvclass"/>
		<Item Name="User.lvclass" Type="LVClass" URL="../client/User/User.lvclass"/>
		<Item Name="Video.lvclass" Type="LVClass" URL="../client/Video/Video.lvclass"/>
		<Item Name="VideoCreateStaging.lvclass" Type="LVClass" URL="../client/VideoCreateStaging/VideoCreateStaging.lvclass"/>
		<Item Name="VideoUpdateStaging.lvclass" Type="LVClass" URL="../client/VideoUpdateStaging/VideoUpdateStaging.lvclass"/>
		<Item Name="Workbook.lvclass" Type="LVClass" URL="../client/Workbook/Workbook.lvclass"/>
		<Item Name="WorkbookCreateStaging.lvclass" Type="LVClass" URL="../client/WorkbookCreateStaging/WorkbookCreateStaging.lvclass"/>
		<Item Name="Workspace.lvclass" Type="LVClass" URL="../client/Workspace/Workspace.lvclass"/>
		<Item Name="Dependencies" Type="Dependencies">
			<Item Name="vi.lib" Type="Folder">
				<Item Name="Error Cluster From Error Code.vi" Type="VI" URL="/&lt;vilib&gt;/Utility/error.llb/Error Cluster From Error Code.vi"/>
			</Item>
			<Item Name="client.lvlib" Type="Library" URL="../client/client.lvlib"/>
			<Item Name="nominalClient_64.dll" Type="Document" URL="../bin/nominalClient_64.dll"/>
		</Item>
		<Item Name="Build Specifications" Type="Build"/>
	</Item>
</Project>
